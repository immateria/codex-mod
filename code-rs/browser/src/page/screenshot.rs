use super::{Page, Screenshot, ScreenshotMode, ScreenshotRegion};

use crate::BrowserError;
use crate::Result;
use crate::config::ImageFormat;
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams;
use std::time::Duration;
use std::time::Instant;
use tracing::debug;
use tracing::info;
use tracing::warn;

impl Page {
    /// Helper function to capture screenshot with retry logic.
    /// Strategy summary (critical to UX and reliability):
    /// - Visible pages: Start with from_surface(false) (no-flash path). If it fails once, retry false quickly.
    ///   Only as a last resort use from_surface(true), because it can flash a visible window.
    /// - Non-visible pages: Use a fast 8×8 preflight (false) to decide. If compositor is unavailable, start
    ///   with from_surface(true) immediately (safe when not visible). Fallbacks stay conservative.
    /// - Final fallback: If two attempts with false fail even while visible, we try true once rather than
    ///   failing entirely. This prevents chronic timeouts; the flash trade-off is acceptable as a last resort.
    ///   Do not loosen these guarantees casually; they were tuned to balance reliability and no-flash UX.
    async fn capture_screenshot_with_retry(
        &self,
        params_builder: chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParamsBuilder,
    ) -> Result<chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotReturns> {
        // Determine page visibility once to decide if from_surface(true) is safe/necessary.
        // If this check fails, assume visible to avoid accidentally picking the flashing path.
        let is_visible = {
            let eval = self
                .cdp_page
                .evaluate(
                    "(() => { try { return { hidden: !!document.hidden, vs: String(document.visibilityState||'unknown') }; } catch (e) { return { hidden: null, vs: 'error' }; } })()",
                )
                .await;
            match eval {
                Ok(v) => {
                    let obj = v.value().cloned().unwrap_or(serde_json::Value::Null);
                    let hidden = obj
                        .get("hidden")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let vs = obj.get("vs").and_then(|x| x.as_str()).unwrap_or("visible");
                    !(hidden || vs != "visible")
                }
                Err(_) => true, // assume visible to avoid risky from_surface(true)
            }
        };

        // Preflight probe (non-visible only):
        // - A very fast 8×8 clip via from_surface(false) predicts if the compositor path is currently viable.
        // - Only run when page is not visible (no flash risk) and cache the result for ~5s.
        // This avoids long timeouts on minimized windows without reintroducing flash for visible ones.
        let mut prefer_false = true;
        if !is_visible {
            let now = Instant::now();
            {
                let mut cache = self.preflight_cache.lock().await;
                if let Some((ts, ok)) = *cache {
                    if now.duration_since(ts) < Duration::from_secs(5) {
                        prefer_false = ok;
                    } else {
                        *cache = None;
                    }
                }
            }

            if prefer_false {
                let cached = {
                    let cache = self.preflight_cache.lock().await;
                    cache.is_some()
                };
                if !cached {
                    let probe_params = params_builder
                        .clone()
                        .from_surface(false)
                        .capture_beyond_viewport(true)
                        .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                            x: 0.0,
                            y: 0.0,
                            width: 8.0,
                            height: 8.0,
                            scale: 1.0,
                        })
                        .build();
                    let probe = tokio::time::timeout(Duration::from_millis(350), self.cdp_page.execute(probe_params)).await;
                    let ok = matches!(probe, Ok(Ok(_)));
                    let mut cache = self.preflight_cache.lock().await;
                    *cache = Some((Instant::now(), ok));
                    prefer_false = ok;
                    if !prefer_false {
                        debug!("Preflight suggests compositor path unavailable; non-visible context will use from_surface(true)");
                    }
                }
            }
        }

        // First attempt policy:
        // - Visible: Always start with from_surface(false) and a short timeout to minimize flash risk.
        // - Not visible: Use preflight outcome; allow from_surface(true) immediately when compositor is unavailable.
        let (first_params, first_timeout, first_is_false) = if is_visible {
            (params_builder.clone().from_surface(false).build(), Duration::from_secs(3), true)
        } else if prefer_false {
            (params_builder.clone().from_surface(false).build(), Duration::from_secs(6), true)
        } else {
            (params_builder.clone().from_surface(true).build(), Duration::from_secs(6), false)
        };
        let first_attempt = tokio::time::timeout(first_timeout, self.cdp_page.execute(first_params)).await;

        match first_attempt {
            Ok(Ok(resp)) => Ok(resp.result),
            Ok(Err(e)) => {
                debug!(
                    "Screenshot first attempt failed (used_false={}): {} (visible={})",
                    first_is_false, e, is_visible
                );
                if !is_visible || !first_is_false {
                    // Non-visible or already tried true path: retry with from_surface(true).
                    // Safe for minimized/hidden windows and avoids repeated long timeouts.
                    let retry_params = params_builder.from_surface(true).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(8),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;

                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(retry_err)) => Err(retry_err.into()),
                        Err(_) => Err(BrowserError::ScreenshotError(
                            "Screenshot retry (from_surface=true) timed out".to_string(),
                        )),
                    }
                } else {
                    // Visible: avoid from_surface(true) if at all possible. Brief wait and retry once with false.
                    tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
                    let retry_params = params_builder.clone().from_surface(false).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(4),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;
                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(_)) => {
                            // Last resort for visible pages: try from_surface(true).
                            // This can flash; we only do it after exhausting the safer path to prevent permanent failures.
                            debug!(
                                "Retry with from_surface(false) failed while visible; attempting from_surface(true) as fallback"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out (final from_surface=true fallback)".to_string(),
                                )),
                            }
                        }
                        Err(_) => {
                            // Timeout on second false attempt; try true once as last resort (may flash)
                            debug!(
                                "Retry with from_surface(false) timed out while visible; attempting from_surface(true) as fallback"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out after retries (from_surface=true fallback)".to_string(),
                                )),
                            }
                        }
                    }
                }
            }
            Err(_) => {
                debug!(
                    "Screenshot first attempt timed out (used_false={}, visible={})",
                    first_is_false, is_visible
                );
                if !is_visible || !first_is_false {
                    // Not visible (safe) or already tried false: try from_surface(true)
                    let retry_params = params_builder.from_surface(true).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(8),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;
                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(e)) => Err(e.into()),
                        Err(_) => Err(BrowserError::ScreenshotError(
                            "Screenshot timed out with from_surface(true)".to_string(),
                        )),
                    }
                } else {
                    // Visible: avoid from_surface(true) if possible; retry quickly with false
                    let retry_params = params_builder.clone().from_surface(false).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(4),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;
                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(_)) => {
                            // Final fallback with from_surface(true) even though visible (see doc rationale)
                            debug!(
                                "Second attempt with from_surface(false) failed while visible; attempting final from_surface(true)"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out after retries (final from_surface=true)".to_string(),
                                )),
                            }
                        }
                        Err(_) => {
                            // Timeout on second false attempt, try true once (final)
                            debug!(
                                "Second attempt with from_surface(false) timed out while visible; attempting final from_surface(true)"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out after retries (from_surface=true fallback)".to_string(),
                                )),
                            }
                        }
                    }
                }
            }
        }
    }

    // (UPDATED) Inject cursor before taking screenshot
    pub async fn screenshot(&self, mode: ScreenshotMode) -> Result<Vec<Screenshot>> {
        // Do not adjust device metrics before screenshots; this causes flashing on
        // external Chrome and adds latency. Rely on connect-time configuration.

        // Fast path: ensure the virtual cursor exists before capturing
        let injected = match self.ensure_virtual_cursor().await {
            Ok(injected) => injected,
            Err(e) => {
                warn!("Failed to inject virtual cursor: {}", e);
                // Continue with screenshot even if cursor injection fails
                false
            }
        };

        // Do not wait for animations to settle; capture current frame to preserve visible motion
        // Small render delay only on fresh injection to avoid empty frame
        if injected {
            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
        }

        match mode {
            ScreenshotMode::Viewport => self.screenshot_viewport().await,
            ScreenshotMode::FullPage { segments_max } => {
                self.screenshot_fullpage(segments_max.unwrap_or(self.config.segments_max))
                    .await
            }
            ScreenshotMode::Region(region) => self.screenshot_region(region).await,
        }
    }

    pub async fn screenshot_viewport(&self) -> Result<Vec<Screenshot>> {
        // Safe viewport capture: do not change device metrics or viewport.
        // Measure CSS viewport size via JS and capture a clipped image
        // using the compositor without affecting focus.
        debug!("Taking viewport screenshot (safe clip, no resize)");

        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        // Probe CSS viewport using Runtime.evaluate to avoid layout_metrics
        let probe = self
            .inject_js(
                "(() => ({ w: (document.documentElement.clientWidth|0), h: (document.documentElement.clientHeight|0) }))()",
            )
            .await
            .unwrap_or(serde_json::Value::Null);

        let doc_w = probe.get("w").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        let doc_h = probe.get("h").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;

        // Fall back to configured viewport if probe failed
        let vw = if doc_w > 0 {
            doc_w
        } else {
            self.config.viewport.width
        };
        let vh = if doc_h > 0 {
            doc_h
        } else {
            self.config.viewport.height
        };

        // Clamp to configured maximums to keep images small for the LLM
        let target_w = vw.min(self.config.viewport.width);
        let target_h = vh.min(self.config.viewport.height);

        let params_builder = CaptureScreenshotParams::builder()
            .format(format)
            .capture_beyond_viewport(true)
            .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                x: 0.0,
                y: 0.0,
                width: target_w as f64,
                height: target_h as f64,
                scale: 1.0,
            });

        // Use our retry logic to handle cases where window is not visible
        let resp = self.capture_screenshot_with_retry(params_builder).await?;
        let data_b64: &str = resp.data.as_ref();
        let data = base64::engine::general_purpose::STANDARD
            .decode(data_b64.as_bytes())
            .map_err(|e| BrowserError::ScreenshotError(format!("base64 decode failed: {e}")))?;

        Ok(vec![Screenshot {
            data,
            width: target_w,
            height: target_h,
            format: self.config.format,
        }])
    }

    pub async fn screenshot_fullpage(&self, segments_max: usize) -> Result<Vec<Screenshot>> {
        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        // 1) Get document dimensions (CSS px)
        let lm = self.cdp_page.layout_metrics().await?;
        let content = lm.css_content_size; // Rect (not Option)
        let doc_w = content.width.ceil() as u32;
        let doc_h = content.height.ceil() as u32;

        // Use your configured viewport width, but never exceed doc width
        let vw = self.config.viewport.width.min(doc_w);
        let vh = self.config.viewport.height;

        // 2) Slice the page by y-offsets WITHOUT scrolling the page
        let mut shots = Vec::new();
        let mut y: u32 = 0;
        let mut taken = 0usize;

        while y < doc_h && taken < segments_max {
            let h = vh.min(doc_h - y); // last slice may be shorter
            let params_builder = CaptureScreenshotParams::builder()
                .format(format.clone())
                .capture_beyond_viewport(true) // key to avoid scrolling/flash
                .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                    x: 0.0,
                    y: y as f64,
                    width: vw as f64,
                    height: h as f64,
                    scale: 1.0,
                });

            let resp = self.capture_screenshot_with_retry(params_builder).await?;
            let data_b64: &str = resp.data.as_ref();
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_b64.as_bytes())
                .map_err(|e| {
                    BrowserError::ScreenshotError(format!("base64 decode failed: {e}"))
                })?;
            shots.push(Screenshot {
                data,
                width: vw,
                height: h,
                format: self.config.format,
            });

            y += h;
            taken += 1;
        }

        if taken == segments_max && y < doc_h {
            info!("[full page truncated at {} segments]", segments_max);
        }

        Ok(shots)
    }

    pub async fn screenshot_region(&self, region: ScreenshotRegion) -> Result<Vec<Screenshot>> {
        debug!(
            "Taking region screenshot: {}x{} at ({}, {})",
            region.width, region.height, region.x, region.y
        );

        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        let params_builder = CaptureScreenshotParams::builder().format(format).clip(
            chromiumoxide::cdp::browser_protocol::page::Viewport {
                x: region.x as f64,
                y: region.y as f64,
                width: region.width as f64,
                height: region.height as f64,
                scale: 1.0,
            },
        );

        let resp = self.capture_screenshot_with_retry(params_builder).await?;
        let data_b64: &str = resp.data.as_ref();
        let data = base64::engine::general_purpose::STANDARD
            .decode(data_b64.as_bytes())
            .map_err(|e| BrowserError::ScreenshotError(format!("base64 decode failed: {e}")))?;

        let final_width = if region.width > 1024 {
            1024
        } else {
            region.width
        };

        Ok(vec![Screenshot {
            data,
            width: final_width,
            height: region.height,
            format: self.config.format,
        }])
    }

}
