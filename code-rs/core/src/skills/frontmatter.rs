/// Extract the YAML frontmatter block from a skill markdown file.
///
/// The expected format is:
/// ```text
/// ---
/// key: value
/// ---
/// (rest of document)
/// ```
pub(crate) fn extract_frontmatter(contents: &str) -> Option<&str> {
    let first_newline = contents.find('\n')?;
    if contents[..first_newline].trim() != "---" {
        return None;
    }

    let yaml_start = first_newline + 1;
    let mut line_start = yaml_start;
    while line_start < contents.len() {
        let rel_end = contents[line_start..].find('\n');
        let line_end = rel_end.map_or(contents.len(), |rel| line_start + rel);
        let line = &contents[line_start..line_end];
        if line.trim() == "---" {
            // Match prior behavior: treat empty frontmatter (---\n---) as missing.
            return (line_start != yaml_start).then(|| &contents[yaml_start..line_start]);
        }

        // If we've reached EOF without finding the closing delimiter, bail.
        if line_end == contents.len() {
            break;
        }

        line_start = line_end + 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::extract_frontmatter;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Frontmatter {
        name: String,
        description: String,
    }

    #[test]
    fn extracts_frontmatter_with_crlf_line_endings() {
        let contents = "---\r\nname: example\r\ndescription: demo\r\n---\r\nbody\r\n";
        let frontmatter = extract_frontmatter(contents).expect("expected frontmatter");
        let parsed: Frontmatter =
            serde_yaml::from_str(frontmatter).expect("expected yaml parse");
        assert_eq!(parsed.name, "example");
        assert_eq!(parsed.description, "demo");
    }

    #[test]
    fn returns_none_for_empty_frontmatter_block() {
        let contents = "---\n---\nbody\n";
        assert!(extract_frontmatter(contents).is_none());
    }
}
