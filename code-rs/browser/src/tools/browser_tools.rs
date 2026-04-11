use crate::Result;
use crate::assets::AssetManager;
use crate::assets::ImageRef;
use crate::config::WaitStrategy;
use crate::manager::BrowserManager;
use crate::page::ScreenshotMode;
use crate::page::ScreenshotRegion;
use crate::page::SetViewportParams;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

pub struct BrowserTools {
    manager: Arc<BrowserManager>,
    asset_manager: Arc<AssetManager>,
}

impl BrowserTools {
    pub fn new(manager: Arc<BrowserManager>, asset_manager: Arc<AssetManager>) -> Self {
        Self {
            manager,
            asset_manager,
        }
    }

    pub async fn handle_tool_call(&self, call: BrowserToolCall) -> Result<BrowserToolResult> {
        match call {
            BrowserToolCall::Goto { url, wait } => {
                let page = self.manager.get_or_create_page().await?;
                let result = page.goto(&url, wait).await?;
                Ok(BrowserToolResult::Goto(result))
            }

            BrowserToolCall::Screenshot {
                mode,
                segments_max,
                region,
                inject_js,
                format: _,
            } => {
                let page = self.manager.get_or_create_page().await?;

                if let Some(script) = inject_js {
                    page.inject_js(&script).await?;
                }

                let screenshot_mode = match mode.as_deref() {
                    Some("full_page") => ScreenshotMode::FullPage { segments_max },
                    Some("viewport") | None => ScreenshotMode::Viewport,
                    Some(_) => {
                        if let Some(r) = region {
                            ScreenshotMode::Region(ScreenshotRegion {
                                x: r.x,
                                y: r.y,
                                width: r.width,
                                height: r.height,
                            })
                        } else {
                            ScreenshotMode::Viewport
                        }
                    }
                };

                let screenshots = page.screenshot(screenshot_mode).await?;
                let ttl_ms = 86_400_000; // 24 hours
                let images = self
                    .asset_manager
                    .store_screenshots(screenshots, ttl_ms)
                    .await?;

                Ok(BrowserToolResult::Screenshot(ScreenshotResult { images }))
            }

            BrowserToolCall::SetViewport {
                width,
                height,
                device_scale_factor,
                mobile,
            } => {
                let page = self.manager.get_or_create_page().await?;
                let result = page
                    .set_viewport(SetViewportParams {
                        width,
                        height,
                        device_scale_factor,
                        mobile,
                    })
                    .await?;
                // Update manager config to reflect the new viewport (no auto-resyncs)
                let dpr = device_scale_factor.unwrap_or(1.0);
                let mob = mobile.unwrap_or(false);
                let _ = self
                    .manager
                    .update_config(|cfg| {
                        cfg.viewport.width = width;
                        cfg.viewport.height = height;
                        cfg.viewport.device_scale_factor = dpr;
                        cfg.viewport.mobile = mob;
                    })
                    .await;
                Ok(BrowserToolResult::SetViewport(result))
            }

            BrowserToolCall::Close { what } => match what.as_deref() {
                Some("browser") => {
                    self.manager.stop().await?;
                    Ok(BrowserToolResult::Close(CloseResult {
                        closed: "browser".to_owned(),
                    }))
                }
                Some("page") | None => {
                    self.manager.close_page().await?;
                    Ok(BrowserToolResult::Close(CloseResult {
                        closed: "page".to_owned(),
                    }))
                }
                Some(other) => Err(crate::BrowserError::ConfigError(format!(
                    "Unknown close target: {other}"
                ))),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum BrowserToolCall {
    #[serde(rename = "browser.goto")]
    Goto {
        url: String,
        wait: Option<WaitStrategy>,
    },

    #[serde(rename = "browser.screenshot")]
    Screenshot {
        mode: Option<String>,
        segments_max: Option<usize>,
        region: Option<RegionParams>,
        inject_js: Option<String>,
        format: Option<String>,
    },

    #[serde(rename = "browser.setViewport")]
    SetViewport {
        width: u32,
        height: u32,
        device_scale_factor: Option<f64>,
        mobile: Option<bool>,
    },

    #[serde(rename = "browser.close")]
    Close { what: Option<String> },
}

#[derive(Debug, Deserialize)]
pub struct RegionParams {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum BrowserToolResult {
    Goto(crate::page::GotoResult),
    Screenshot(ScreenshotResult),
    SetViewport(crate::page::ViewportResult),
    Close(CloseResult),
}

#[derive(Debug, Serialize)]
pub struct ScreenshotResult {
    pub images: Vec<ImageRef>,
}

#[derive(Debug, Serialize)]
pub struct CloseResult {
    pub closed: String,
}
