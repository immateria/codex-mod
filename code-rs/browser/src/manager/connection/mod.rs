mod discovery;
mod external;
mod handler;
mod lifecycle;
mod start;

pub(super) use discovery::discover_ws_via_host_port;
pub(super) use discovery::scan_for_chrome_debug_port;
pub(super) use handler::should_stop_handler;

#[cfg(test)]
pub(super) use discovery::JsonVersion;
#[cfg(test)]
pub(super) use handler::should_restart_handler;
