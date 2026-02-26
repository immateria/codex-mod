use crate::bottom_pane::SettingsSection;
use crate::chrome_launch::ChromeLaunchOption;

use code_core::spawn::spawn_std_command_with_retry;

use super::super::ChatWidget;

impl ChatWidget<'_> {
    pub(crate) fn show_chrome_options(&mut self, port: Option<u16>) {
        self.ensure_settings_overlay_section(SettingsSection::Chrome);
        let content = self.build_chrome_settings_content(port);
        if let Some(overlay) = self.settings.overlay.as_mut() {
            overlay.set_chrome_content(content);
        }
        self.request_redraw();
    }

    pub(crate) fn handle_chrome_launch_option(
        &mut self,
        option: ChromeLaunchOption,
        port: Option<u16>,
    ) {
        let launch_port = port.unwrap_or(super::DEFAULT_CHROME_REMOTE_DEBUG_PORT);
        let ticket = self.make_background_tail_ticket();

        match option {
            ChromeLaunchOption::CloseAndUseProfile => {
                // Kill existing Chrome and launch with user profile
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("pkill")
                        .arg("-f")
                        .arg("Google Chrome")
                        .output();
                    std::thread::sleep(super::CHROME_KILL_SETTLE_DELAY);
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("pkill").arg("-f").arg("chrome").output();
                    std::thread::sleep(super::CHROME_KILL_SETTLE_DELAY);
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("taskkill")
                        .arg("/F")
                        .arg("/IM")
                        .arg("chrome.exe")
                        .output();
                    std::thread::sleep(super::CHROME_KILL_SETTLE_DELAY);
                }
                self.launch_chrome_with_profile(launch_port);
                // Connect to Chrome after launching
                self.connect_to_chrome_after_launch(launch_port, ticket);
            }
            ChromeLaunchOption::UseTempProfile => {
                // Launch with temporary profile
                self.launch_chrome_with_temp_profile(launch_port);
                // Connect to Chrome after launching
                self.connect_to_chrome_after_launch(launch_port, ticket);
            }
            ChromeLaunchOption::UseInternalBrowser => {
                // Redirect to internal browser command
                self.handle_browser_command(String::new());
            }
            ChromeLaunchOption::Cancel => {
                // Do nothing, just close the dialog
            }
        }
    }

    fn launch_chrome_with_profile(&mut self, port: u16) {
        use std::process::Stdio;
        let log_path = self.chrome_log_path();

        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new(
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            );
            cmd.arg(format!("--remote-debugging-port={port}"))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-component-extensions-with-background-pages")
                .arg("--disable-background-networking")
                .arg("--silent-debugger-extension-api")
                .arg("--remote-allow-origins=*")
                .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                .arg("--disable-hang-monitor")
                .arg("--disable-background-timer-throttling")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null());
            self.apply_chrome_logging(&mut cmd, log_path.as_deref());
            if let Err(err) = spawn_std_command_with_retry(&mut cmd) {
                tracing::warn!("failed to launch Chrome with profile: {err}");
            }
        }

        #[cfg(target_os = "linux")]
        {
            let mut cmd = std::process::Command::new("google-chrome");
            cmd.arg(format!("--remote-debugging-port={port}"))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-component-extensions-with-background-pages")
                .arg("--disable-background-networking")
                .arg("--silent-debugger-extension-api")
                .arg("--remote-allow-origins=*")
                .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                .arg("--disable-hang-monitor")
                .arg("--disable-background-timer-throttling")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null());
            self.apply_chrome_logging(&mut cmd, log_path.as_deref());
            if let Err(err) = spawn_std_command_with_retry(&mut cmd) {
                tracing::warn!("failed to launch Chrome with profile: {err}");
            }
        }

        #[cfg(target_os = "windows")]
        {
            let chrome_paths = vec![
                "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe".to_string(),
                "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe".to_string(),
                format!(
                    "{}\\AppData\\Local\\Google\\Chrome\\Application\\chrome.exe",
                    std::env::var("USERPROFILE").unwrap_or_default()
                ),
            ];

            for chrome_path in chrome_paths {
                if std::path::Path::new(&chrome_path).exists() {
                    let mut cmd = std::process::Command::new(&chrome_path);
                    cmd.arg(format!("--remote-debugging-port={port}"))
                        .arg("--no-first-run")
                        .arg("--no-default-browser-check")
                        .arg("--disable-component-extensions-with-background-pages")
                        .arg("--disable-background-networking")
                        .arg("--silent-debugger-extension-api")
                        .arg("--remote-allow-origins=*")
                        .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                        .arg("--disable-hang-monitor")
                        .arg("--disable-background-timer-throttling")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .stdin(Stdio::null());
                    self.apply_chrome_logging(&mut cmd, log_path.as_deref());
                    if let Err(err) = spawn_std_command_with_retry(&mut cmd) {
                        tracing::warn!("failed to launch Chrome with profile: {err}");
                    }
                    break;
                }
            }
        }

        // Add status message
        self.push_background_tail("Chrome launched with user profile".to_string());
        // Show browsing state in input border after launch
        self.bottom_pane.update_status_text("using browser".to_string());
    }

    fn launch_chrome_with_temp_profile(&mut self, port: u16) {
        use std::process::Stdio;

        let temp_dir = std::env::temp_dir();
        let profile_dir = temp_dir.join(format!("code-chrome-temp-{port}"));
        let log_path = self.chrome_log_path();

        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new(
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            );
            cmd.arg(format!("--remote-debugging-port={port}"))
                .arg(format!("--user-data-dir={}", profile_dir.display()))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-component-extensions-with-background-pages")
                .arg("--disable-background-networking")
                .arg("--silent-debugger-extension-api")
                .arg("--remote-allow-origins=*")
                .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                .arg("--disable-hang-monitor")
                .arg("--disable-background-timer-throttling")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null());
            self.apply_chrome_logging(&mut cmd, log_path.as_deref());
            if let Err(err) = spawn_std_command_with_retry(&mut cmd) {
                tracing::warn!("failed to launch Chrome with temp profile: {err}");
            }
        }

        #[cfg(target_os = "linux")]
        {
            let mut cmd = std::process::Command::new("google-chrome");
            cmd.arg(format!("--remote-debugging-port={port}"))
                .arg(format!("--user-data-dir={}", profile_dir.display()))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-component-extensions-with-background-pages")
                .arg("--disable-background-networking")
                .arg("--silent-debugger-extension-api")
                .arg("--remote-allow-origins=*")
                .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                .arg("--disable-hang-monitor")
                .arg("--disable-background-timer-throttling")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null());
            self.apply_chrome_logging(&mut cmd, log_path.as_deref());
            if let Err(err) = spawn_std_command_with_retry(&mut cmd) {
                tracing::warn!("failed to launch Chrome with temp profile: {err}");
            }
        }

        #[cfg(target_os = "windows")]
        {
            let chrome_paths = vec![
                "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe".to_string(),
                "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe".to_string(),
                format!(
                    "{}\\AppData\\Local\\Google\\Chrome\\Application\\chrome.exe",
                    std::env::var("USERPROFILE").unwrap_or_default()
                ),
            ];

            for chrome_path in chrome_paths {
                if std::path::Path::new(&chrome_path).exists() {
                    let mut cmd = std::process::Command::new(&chrome_path);
                    cmd.arg(format!("--remote-debugging-port={port}"))
                        .arg(format!("--user-data-dir={}", profile_dir.display()))
                        .arg("--no-first-run")
                        .arg("--no-default-browser-check")
                        .arg("--disable-component-extensions-with-background-pages")
                        .arg("--disable-background-networking")
                        .arg("--silent-debugger-extension-api")
                        .arg("--remote-allow-origins=*")
                        .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                        .arg("--disable-hang-monitor")
                        .arg("--disable-background-timer-throttling")
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .stdin(Stdio::null());
                    self.apply_chrome_logging(&mut cmd, log_path.as_deref());
                    if let Err(err) = spawn_std_command_with_retry(&mut cmd) {
                        tracing::warn!("failed to launch Chrome with temp profile: {err}");
                    }
                    break;
                }
            }
        }

        // Add status message
        self.push_background_tail(format!(
            "Chrome launched with temporary profile at {}",
            profile_dir.display()
        ));
    }

    fn chrome_log_path(&self) -> Option<String> {
        if !self.config.debug {
            return None;
        }
        let log_dir = code_core::config::log_dir(&self.config).ok()?;
        Some(log_dir.join("code-chrome.log").display().to_string())
    }

    fn apply_chrome_logging(&self, cmd: &mut std::process::Command, log_path: Option<&str>) {
        if let Some(path) = log_path {
            cmd.arg("--enable-logging")
                .arg("--log-level=1")
                .arg(format!("--log-file={path}"));
        }
    }
}
