#[cfg(target_os = "android")]
compile_error!(
    "Browser automation is not supported on Android/Termux.\n\
Build the CLI with:\n\
  cargo build -p code-cli --no-default-features\n"
);

pub mod assets;
pub mod config;
pub mod global;
pub mod hooks;
pub mod manager;
pub mod page;
pub mod tools;

pub use config::BrowserConfig;
pub use config::ViewportConfig;
pub use config::WaitStrategy;
pub use manager::BrowserManager;
pub use page::Page;
pub use page::ScreenshotMode;
pub use page::ScreenshotRegion;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("Browser not initialized")]
    NotInitialized,

    #[error("Page not loaded")]
    PageNotLoaded,

    #[error("CDP error: {0}")]
    CdpError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Screenshot failed: {0}")]
    ScreenshotError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Asset storage error: {0}")]
    AssetError(String),
}

impl From<chromiumoxide::error::CdpError> for BrowserError {
    fn from(e: chromiumoxide::error::CdpError) -> Self {
        BrowserError::CdpError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BrowserError>;
