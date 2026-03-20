mod action;
mod inline_scripts;
mod read_annotation;
mod render;
mod shell_display;

pub(crate) use action::{action_enum_from_parsed, first_context_path};
pub(crate) use inline_scripts::format_inline_script_for_display;
pub(crate) use read_annotation::{
    coalesce_read_ranges_in_lines_local,
    parse_read_line_annotation,
    parse_read_line_annotation_with_range,
};
pub(crate) use render::{
    exec_command_lines,
    exec_render_parts_parsed,
    exec_render_parts_parsed_with_meta,
    running_status_line,
};
pub(crate) use shell_display::{
    emphasize_shell_command_name,
    insert_line_breaks_after_double_ampersand,
    normalize_shell_command_display,
};

