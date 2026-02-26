use std::time::Duration;

mod args;
mod connect;
mod launch;
mod screenshots;

const DEFAULT_CHROME_REMOTE_DEBUG_PORT: u16 = 9222;
const CHROME_KILL_SETTLE_DELAY: Duration = Duration::from_millis(500);
const CHROME_LAUNCH_CONNECT_DELAY: Duration = Duration::from_secs(2);
const CDP_CONNECT_DEADLINE: Duration = Duration::from_secs(20);
const CDP_SCREENSHOT_DELAY: Duration = Duration::from_millis(250);
const CDP_SCREENSHOT_MAX_ATTEMPTS: usize = 2;
const BROWSER_INITIAL_SCREENSHOT_DELAY: Duration = Duration::from_millis(300);

#[cfg(test)]
mod tests;

