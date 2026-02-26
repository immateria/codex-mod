use crate::config::BrowserConfig;
use crate::page::Page;
use chromiumoxide::Browser;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::Instant;

mod cdp;
mod connection;
mod cleanup;
mod input;
mod monitor;
mod navigation;
mod pages;
mod page_ops;
mod screenshot;
mod status;
mod targets;

type NavigationCallback = Box<dyn Fn(String) + Send + Sync>;
type NavigationCallbackSlot = Arc<RwLock<Option<NavigationCallback>>>;
type LastAppliedMetrics = Option<(i64, i64, f64, bool, std::time::Instant)>;
type LastAppliedMetricsSlot = Arc<Mutex<LastAppliedMetrics>>;

pub struct BrowserManager {
    pub config: Arc<RwLock<BrowserConfig>>,
    browser: Arc<Mutex<Option<Browser>>>,
    page: Arc<Mutex<Option<Arc<Page>>>>,
    // Dedicated background page for screenshots to prevent focus stealing
    background_page: Arc<Mutex<Option<Arc<Page>>>>,
    last_activity: Arc<Mutex<Instant>>,
    idle_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    event_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    assets: Arc<Mutex<Option<Arc<crate::assets::AssetManager>>>>,
    user_data_dir: Arc<Mutex<Option<String>>>,
    cleanup_profile_on_drop: Arc<Mutex<bool>>,
    navigation_callback: NavigationCallbackSlot,
    navigation_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    viewport_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Gate to temporarily disable all automatic viewport corrections (post-initial set)
    auto_viewport_correction_enabled: Arc<tokio::sync::RwLock<bool>>,
    /// Track last applied device metrics to avoid redundant overrides
    last_metrics_applied: LastAppliedMetricsSlot,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserTargetSummary {
    pub index: usize,
    pub target_id: String,
    pub title: String,
    pub url: String,
    pub attached: bool,
    pub opener_id: Option<String>,
    pub subtype: Option<String>,
    pub controllable: bool,
    pub active: bool,
}

impl BrowserManager {
    const SCREENSHOT_TTL_MS: u64 = 86_400_000; // 24 hours

    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            browser: Arc::new(Mutex::new(None)),
            page: Arc::new(Mutex::new(None)),
            background_page: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            idle_monitor_handle: Arc::new(Mutex::new(None)),
            event_task: Arc::new(Mutex::new(None)),
            assets: Arc::new(Mutex::new(None)),
            user_data_dir: Arc::new(Mutex::new(None)),
            cleanup_profile_on_drop: Arc::new(Mutex::new(false)),
            navigation_callback: Arc::new(tokio::sync::RwLock::new(None)),
            navigation_monitor_handle: Arc::new(Mutex::new(None)),
            viewport_monitor_handle: Arc::new(Mutex::new(None)),
            auto_viewport_correction_enabled: Arc::new(tokio::sync::RwLock::new(true)),
            last_metrics_applied: Arc::new(Mutex::new(None)),
        }
    }

}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BrowserStatus {
    pub enabled: bool,
    pub browser_active: bool,
    pub current_url: Option<String>,
    pub viewport: crate::config::ViewportConfig,
    pub fullpage: bool,
}

#[cfg(test)]
mod tests;
