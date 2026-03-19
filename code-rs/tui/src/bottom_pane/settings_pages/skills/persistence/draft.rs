use super::*;

pub(super) fn generate_draft_inner(view: &mut SkillsSettingsView) {
    let name = view.editor.name_field.text().trim().to_string();
    let description = view.editor.description_field.text().trim().to_string();
    if let Err(msg) = view.validate_name(&name) {
        view.status = Some((msg, Style::default().fg(colors::error())));
        return;
    }
    if let Err(msg) = view.validate_description(&description) {
        view.status = Some((msg, Style::default().fg(colors::error())));
        return;
    }

    let shell_style = view.editor.style_field.text().trim();
    let trigger_examples = view.editor.examples_field.text().trim();
    let title = name.replace('-', " ");

    let mut body = format!(
        "# {title}\n\n## Purpose\n\n{description}\n\n## Workflow\n\n1. Describe the first deterministic step.\n2. Describe conditional branches and constraints.\n3. Point to scripts/references/assets when needed.\n"
    );

    if !trigger_examples.is_empty() {
        body.push_str("\n## Trigger Examples\n\n");
        body.push_str(trigger_examples);
        body.push('\n');
    }

    if !shell_style.is_empty() {
        body.push_str("\n## Shell Style Integration\n\n");
        body.push_str(
            "This skill is intended for shell-style-aware loading. Configure it under `shell_style_profiles` when appropriate.\n\n",
        );
        body.push_str(&format!(
            "- Preferred shell style: `{shell_style}`\n- Consider wiring via `shell_style_profiles.{shell_style}.skill_roots`\n"
        ));
    }

    view.editor.body_field.set_text(&body);
    view.status = Some((
        "Draft generated from guided fields. Review and Save.".to_string(),
        Style::default().fg(colors::success()),
    ));
}

