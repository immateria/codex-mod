use super::*;

pub(super) fn handle_paste_direct_inner(view: &mut SkillsSettingsView, text: String) -> bool {
    match view.editor.focus {
        Focus::Name => {
            view.editor.name_field.handle_paste(text);
            true
        }
        Focus::Description => {
            view.editor.description_field.handle_paste(text);
            true
        }
        Focus::Style => {
            let previous_style = ShellScriptStyle::parse(view.editor.style_field.text().trim());
            view.editor.style_field.handle_paste(text);
            style_profile::sync_style_profile_fields_if_needed(view, previous_style);
            true
        }
        Focus::StyleReferences => {
            let before = view.editor.style_references_field.text().to_string();
            view.editor.style_references_field.handle_paste(text);
            if view.editor.style_references_field.text() != before {
                view.editor.style_references_dirty = true;
            }
            true
        }
        Focus::StyleSkillRoots => {
            let before = view.editor.style_skill_roots_field.text().to_string();
            view.editor.style_skill_roots_field.handle_paste(text);
            if view.editor.style_skill_roots_field.text() != before {
                view.editor.style_skill_roots_dirty = true;
            }
            true
        }
        Focus::StyleMcpInclude => {
            let before = view.editor.style_mcp_include_field.text().to_string();
            view.editor.style_mcp_include_field.handle_paste(text);
            if view.editor.style_mcp_include_field.text() != before {
                view.editor.style_mcp_include_dirty = true;
            }
            true
        }
        Focus::StyleMcpExclude => {
            let before = view.editor.style_mcp_exclude_field.text().to_string();
            view.editor.style_mcp_exclude_field.handle_paste(text);
            if view.editor.style_mcp_exclude_field.text() != before {
                view.editor.style_mcp_exclude_dirty = true;
            }
            true
        }
        Focus::Examples => {
            view.editor.examples_field.handle_paste(text);
            true
        }
        Focus::Body => {
            view.editor.body_field.handle_paste(text);
            true
        }
        Focus::StyleProfile
        | Focus::Generate
        | Focus::Save
        | Focus::Delete
        | Focus::Cancel
        | Focus::List => false,
    }
}

