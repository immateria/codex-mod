use super::{Page, SetViewportParams, ViewportResult};

use crate::BrowserError;
use crate::Result;
use crate::ViewportConfig;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;

impl Page {
    pub async fn set_viewport(&self, viewport: SetViewportParams) -> Result<ViewportResult> {
        // Apply CDP device metrics override once on demand
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(i64::from(viewport.width))
            .height(i64::from(viewport.height))
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

    pub fn update_viewport(&self, _viewport: ViewportConfig) -> Result<()> {
        Ok(())
    }

}
