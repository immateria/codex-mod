use super::{Page, SetViewportParams, ViewportResult};

use crate::BrowserError;
use crate::Result;
use crate::ViewportConfig;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use tracing::debug;
use tracing::info;

impl Page {
    /// Check and fix viewport scaling issues before taking screenshots
    #[allow(dead_code)]
    async fn check_and_fix_scaling(&self) -> Result<()> {
        // Never touch viewport metrics for external Chrome connections.
        // Changing device metrics on a user's Chrome causes a visible flash
        // and slows down screenshots. We only verify/correct for internally
        // launched Chrome where we control the window.
        if self.config.connect_port.is_some() || self.config.connect_ws.is_some() {
            return Ok(());
        }
        // Check current viewport and scaling
        let check_script = r#"
            (() => {
                const vw = window.innerWidth;
                const vh = window.innerHeight;
                const dpr = window.devicePixelRatio || 1;
                const zoom = Math.round(window.outerWidth / window.innerWidth * 100) / 100;
                
                // Check if viewport matches expected dimensions
                const expectedWidth = %EXPECTED_WIDTH%;
                const expectedHeight = %EXPECTED_HEIGHT%;
                const expectedDpr = %EXPECTED_DPR%;
                
                return {
                    currentWidth: vw,
                    currentHeight: vh,
                    currentDpr: dpr,
                    currentZoom: zoom,
                    expectedWidth: expectedWidth,
                    expectedHeight: expectedHeight,
                    expectedDpr: expectedDpr,
                    // Only correct when there's a meaningful mismatch in size/DPR.
                    // Ignore zoom heuristics which can be noisy on some platforms.
                    needsCorrection: (
                        Math.abs(vw - expectedWidth) > 5 ||
                        Math.abs(vh - expectedHeight) > 5 ||
                        Math.abs(dpr - expectedDpr) > 0.05
                    )
                };
            })()
        "#;

        // Replace placeholders with actual expected values
        let script = check_script
            .replace("%EXPECTED_WIDTH%", &self.config.viewport.width.to_string())
            .replace(
                "%EXPECTED_HEIGHT%",
                &self.config.viewport.height.to_string(),
            )
            .replace(
                "%EXPECTED_DPR%",
                &self.config.viewport.device_scale_factor.to_string(),
            );

        let result = self.cdp_page.evaluate(script).await?;

        if let Some(obj) = result.value() {
            let needs_correction = obj
                .get("needsCorrection")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            if needs_correction {
                let current_width = obj
                    .get("currentWidth")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let current_height = obj
                    .get("currentHeight")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let current_dpr = obj
                    .get("currentDpr")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0);
                let current_zoom = obj
                    .get("currentZoom")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0);

                debug!(
                    "Viewport needs correction: {}x{} @ {}x DPR (zoom: {}) -> {}x{} @ {}x DPR",
                    current_width,
                    current_height,
                    current_dpr,
                    current_zoom,
                    self.config.viewport.width,
                    self.config.viewport.height,
                    self.config.viewport.device_scale_factor
                );

                // Use CDP to set the correct viewport metrics
                let params = SetDeviceMetricsOverrideParams::builder()
                    .width(self.config.viewport.width as i64)
                    .height(self.config.viewport.height as i64)
                    .device_scale_factor(self.config.viewport.device_scale_factor)
                    .mobile(self.config.viewport.mobile)
                    .build()
                    .map_err(|e| {
                        BrowserError::CdpError(format!("Failed to build viewport params: {e}"))
                    })?;

                self.cdp_page.execute(params).await?;

                // Avoid aggressive zoom resets to reduce reflow/flash.
                // If internal zoom is off, leave it unless size/DPR corrected above isn't sufficient.

                info!("Viewport scaling corrected");
            }
        }

        Ok(())
    }

    pub async fn set_viewport(&self, viewport: SetViewportParams) -> Result<ViewportResult> {
        // Apply CDP device metrics override once on demand
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(viewport.width as i64)
            .height(viewport.height as i64)
            .device_scale_factor(viewport.device_scale_factor.unwrap_or(1.0))
            .mobile(viewport.mobile.unwrap_or(false))
            .build()
            .map_err(|e| BrowserError::CdpError(format!("Failed to build viewport params: {e}")))?;
        self.cdp_page.execute(params).await?;

        Ok(ViewportResult {
            width: viewport.width,
            height: viewport.height,
            dpr: viewport.device_scale_factor.unwrap_or(1.0),
        })
    }

    pub async fn update_viewport(&self, _viewport: ViewportConfig) -> Result<()> {
        Ok(())
    }

}
