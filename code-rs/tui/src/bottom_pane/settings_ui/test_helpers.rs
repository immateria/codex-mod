use std::fmt::Debug;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub(crate) fn assert_layout_and_render_shell_agree<Layout>(
    area: Rect,
    layout: impl FnOnce(Rect) -> Option<Layout>,
    render_shell: impl FnOnce(Rect, &mut Buffer) -> Option<Layout>,
) -> Layout
where
    Layout: Debug + PartialEq,
{
    let expected = layout(area).expect("layout");
    let mut buf = Buffer::empty(area);
    let rendered = render_shell(area, &mut buf).expect("render_shell");
    assert_eq!(expected, rendered);
    expected
}
