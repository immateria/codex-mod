use crate::chrome_launch::ChromeLaunchOption;

use super::ChatWidget;

impl ChatWidget<'_> {
    pub(crate) fn show_chrome_options(&mut self, _port: Option<u16>) {
        self.flash_footer_notice("Chrome/CDP is not available on Android builds.".to_string());
    }

    pub(crate) fn handle_chrome_launch_option(
        &mut self,
        _option: ChromeLaunchOption,
        _port: Option<u16>,
    ) {
        self.flash_footer_notice("Chrome/CDP is not available on Android builds.".to_string());
    }

    pub(crate) fn handle_chrome_command(&mut self, _command_text: String) {
        self.flash_footer_notice("Chrome/CDP is not available on Android builds.".to_string());
    }
}

