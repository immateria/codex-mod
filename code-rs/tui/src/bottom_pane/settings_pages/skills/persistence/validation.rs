use super::*;

impl SkillsSettingsView {
    pub(super) fn validate_name(&self, name: &str) -> Result<(), String> {
        let slug = name.trim();
        if slug.is_empty() {
            return Err("Name is required".to_string());
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err("Name must use letters, numbers, '-', '_' or '.'".to_string());
        }

        let selected = self.selected_list_index();
        let dup = self
            .skills
            .iter()
            .enumerate()
            .any(|(idx, skill)| idx != selected && skill_slug(skill).eq_ignore_ascii_case(slug));
        if dup {
            return Err("A skill with this name already exists".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_frontmatter(&self, body: &str) -> Result<(), String> {
        if extract_frontmatter(body).is_none() {
            return Err("SKILL.md must start with YAML frontmatter".to_string());
        }
        if frontmatter_value(body, "name").is_none() {
            return Err("Frontmatter must include name".to_string());
        }
        if frontmatter_value(body, "description").is_none() {
            return Err("Frontmatter must include description".to_string());
        }
        Ok(())
    }

    pub(super) fn validate_description(&self, description: &str) -> Result<(), String> {
        if description.trim().is_empty() {
            return Err("Description is required".to_string());
        }
        Ok(())
    }
}

