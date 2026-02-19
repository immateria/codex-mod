use code_core::protocol::ReviewOutputEvent;
use code_core::protocol::ReviewSnapshotInfo;
use code_protocol::models::ContentItem;
use code_protocol::models::ResponseItem;
use std::path::PathBuf;

pub(crate) fn build_fix_prompt(review: &ReviewOutputEvent) -> String {
    let summary = format_review_findings(review);
    let raw_json = serde_json::to_string_pretty(review).unwrap_or_else(|_| "{}".to_string());
    let mut preface = String::from(
        "You are continuing an automated /review resolution loop. Review the listed findings and determine whether they represent real issues introduced by our changes. If they are, apply the necessary fixes and resolve any similar issues you can identify before responding."
    );
    if !summary.is_empty() {
        preface.push_str("\n\nFindings:\n");
        preface.push_str(&summary);
    }
    preface.push_str("\n\nFull review JSON (includes file paths and line ranges):\n");
    preface.push_str(&raw_json);
    format!(
        "Is this a real issue introduced by our changes? If so, please fix and resolve all similar issues.\n\n{preface}"
    )
}

pub(crate) fn format_review_findings(output: &ReviewOutputEvent) -> String {
    if output.findings.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for (idx, f) in output.findings.iter().enumerate() {
        let title = f.title.trim();
        let body = f.body.trim();
        let location = format!(
            "path: {}:{}-{}",
            f.code_location
                .absolute_file_path
                .to_string_lossy(),
            f.code_location.line_range.start,
            f.code_location.line_range.end
        );
        if body.is_empty() {
            parts.push(format!("{}. {}\n{}", idx + 1, title, location));
        } else {
            parts.push(format!("{}. {}\n{}\n{}", idx + 1, title, location, body));
        }
    }
    parts.join("\n\n")
}

pub(crate) fn review_summary_line(output: &ReviewOutputEvent) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let explanation = output.overall_explanation.trim();
    if !explanation.is_empty() {
        parts.push(explanation.to_string());
    }

    if !output.findings.is_empty() {
        let titles: Vec<String> = output
            .findings
            .iter()
            .filter_map(|f| {
                let title = f.title.trim();
                (!title.is_empty()).then_some(title.to_string())
            })
            .collect();
        if !titles.is_empty() {
            parts.push(format!("Findings: {}", titles.join("; ")));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" \n"))
    }
}

pub(crate) fn make_user_message(text: String) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text }],
        end_turn: None,
        phase: None,
    }
}

pub(crate) fn make_assistant_message(text: String) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        end_turn: None,
        phase: None,
    }
}

pub(crate) fn write_review_json(
    path: PathBuf,
    outputs: &[ReviewOutputEvent],
    snapshot: Option<&ReviewSnapshotInfo>,
) -> std::io::Result<()> {
    if outputs.is_empty() {
        return Ok(());
    }

    #[derive(serde::Serialize)]
    struct ReviewRun<'a> {
        index: usize,
        #[serde(flatten)]
        output: &'a ReviewOutputEvent,
    }

    #[derive(serde::Serialize)]
    struct ReviewJsonOutput<'a> {
        #[serde(flatten)]
        latest: &'a ReviewOutputEvent,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        runs: Vec<ReviewRun<'a>>,
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        snapshot: Option<&'a ReviewSnapshotInfo>,
    }

    let Some(latest) = outputs.last() else {
        return Ok(());
    };
    let runs: Vec<ReviewRun<'_>> = outputs
        .iter()
        .enumerate()
        .map(|(idx, output)| ReviewRun {
            index: idx + 1,
            output,
        })
        .collect();

    let payload = ReviewJsonOutput {
        latest,
        runs,
        snapshot,
    };
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, json)
}
