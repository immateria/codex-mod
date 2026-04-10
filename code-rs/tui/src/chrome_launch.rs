#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChromeLaunchOption {
    CloseAndUseProfile,
    UseTempProfile,
    UseInternalBrowser,
    Cancel,
}

pub(crate) const CHROME_LAUNCH_CHOICES: &[(ChromeLaunchOption, &str, &str)] = &[
    (
        ChromeLaunchOption::CloseAndUseProfile,
        "Close existing Chrome & use your profile",
        "Closes any running Chrome and launches with your profile",
    ),
    (
        ChromeLaunchOption::UseTempProfile,
        "Use temporary profile",
        "Launches Chrome with a clean profile (no saved logins)",
    ),
    (
        ChromeLaunchOption::UseInternalBrowser,
        "Use internal browser (/browser)",
        "Uses the built-in browser instead of Chrome",
    ),
    (
        ChromeLaunchOption::Cancel,
        "Cancel",
        "Don't launch any browser",
    ),
];
