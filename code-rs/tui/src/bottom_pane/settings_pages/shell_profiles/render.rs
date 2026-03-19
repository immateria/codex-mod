use super::*;

pub(super) fn render_in_chrome(
    view: &ShellProfilesSettingsView,
    chrome: ChromeMode,
    area: Rect,
    buf: &mut Buffer,
) {
    match &view.mode {
        ViewMode::Main => match chrome {
            ChromeMode::Framed => view.render_main(area, buf),
            ChromeMode::ContentOnly => view.render_main_without_frame(area, buf),
        },
        ViewMode::EditList { target, .. } => match chrome {
            ChromeMode::Framed => view.render_editor(area, buf, *target),
            ChromeMode::ContentOnly => view.render_editor_without_frame(area, buf, *target),
        },
        ViewMode::PickList(state) => match chrome {
            ChromeMode::Framed => view.render_picker(area, buf, state),
            ChromeMode::ContentOnly => view.render_picker_without_frame(area, buf, state),
        },
    }
}
