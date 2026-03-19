use crate::protocol::types::{ReviewFinding, ReviewOutputEvent};

fn format_location(item: &ReviewFinding) -> String {
    let path = item.code_location.absolute_file_path.display();
    let start = item.code_location.line_range.start;
    let end = item.code_location.line_range.end;
    format!("{path}:{start}-{end}")
}

const REVIEW_FALLBACK_MESSAGE: &str = "Reviewer failed to output a response.";

/// Format a full review findings block as plain text lines.
///
/// - When `selection` is `Some`, each item line includes a checkbox marker:
///   "[x]" for selected items and "[ ]" for unselected. Missing indices
///   default to selected.
/// - When `selection` is `None`, the marker is omitted and a simple bullet is
///   rendered ("- Title — path:start-end").
pub fn format_review_findings_block(
    findings: &[ReviewFinding],
    selection: Option<&[bool]>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(String::new());

    if findings.len() > 1 {
        lines.push("Full review comments:".to_string());
    } else {
        lines.push("Review comment:".to_string());
    }

    for (idx, item) in findings.iter().enumerate() {
        lines.push(String::new());

        let title = &item.title;
        let location = format_location(item);

        if let Some(flags) = selection {
            let checked = flags.get(idx).copied().unwrap_or(true);
            let marker = if checked { "[x]" } else { "[ ]" };
            lines.push(format!("- {marker} {title} — {location}"));
        } else {
            lines.push(format!("- {title} — {location}"));
        }

        for body_line in item.body.lines() {
            lines.push(format!("  {body_line}"));
        }
    }

    lines.join("\n")
}

/// Render a human-readable review summary suitable for a user-facing message.
///
/// Returns either the explanation, the formatted findings block, or both
/// separated by a blank line. If neither is present, emits a fallback message.
pub fn render_review_output_text(output: &ReviewOutputEvent) -> String {
    let mut sections = Vec::new();
    let explanation = output.overall_explanation.trim();
    if !explanation.is_empty() {
        sections.push(explanation.to_string());
    }
    if !output.findings.is_empty() {
        let findings = format_review_findings_block(&output.findings, None);
        let trimmed = findings.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
    if sections.is_empty() {
        REVIEW_FALLBACK_MESSAGE.to_string()
    } else {
        sections.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{ReviewCodeLocation, ReviewLineRange};
    use std::path::PathBuf;

    fn make_finding(title: &str, body: &str, path: &str, start: u32, end: u32) -> ReviewFinding {
        ReviewFinding {
            title: title.to_string(),
            body: body.to_string(),
            confidence_score: 0.9,
            priority: 1,
            code_location: ReviewCodeLocation {
                absolute_file_path: PathBuf::from(path),
                line_range: ReviewLineRange { start, end },
            },
        }
    }

    #[test]
    fn single_finding_no_selection() {
        let findings = vec![make_finding("Bug A", "Details here.", "/tmp/a.rs", 1, 5)];
        let text = format_review_findings_block(&findings, None);
        assert!(text.contains("Review comment:"));
        assert!(text.contains("- Bug A — /tmp/a.rs:1-5"));
        assert!(text.contains("  Details here."));
    }

    #[test]
    fn multiple_findings_no_selection() {
        let findings = vec![
            make_finding("Bug A", "A body.", "/tmp/a.rs", 1, 5),
            make_finding("Bug B", "B body.", "/tmp/b.rs", 10, 20),
        ];
        let text = format_review_findings_block(&findings, None);
        assert!(text.contains("Full review comments:"));
        assert!(text.contains("- Bug A — /tmp/a.rs:1-5"));
        assert!(text.contains("- Bug B — /tmp/b.rs:10-20"));
    }

    #[test]
    fn findings_with_selection() {
        let findings = vec![
            make_finding("Bug A", "A body.", "/tmp/a.rs", 1, 5),
            make_finding("Bug B", "B body.", "/tmp/b.rs", 10, 20),
        ];
        let text = format_review_findings_block(&findings, Some(&[true, false]));
        assert!(text.contains("[x] Bug A"));
        assert!(text.contains("[ ] Bug B"));
    }

    #[test]
    fn selection_defaults_to_checked_for_missing_indices() {
        let findings = vec![
            make_finding("Bug A", "A.", "/tmp/a.rs", 1, 2),
            make_finding("Bug B", "B.", "/tmp/b.rs", 3, 4),
        ];
        // Only one flag provided; second defaults to selected.
        let text = format_review_findings_block(&findings, Some(&[false]));
        assert!(text.contains("[ ] Bug A"));
        assert!(text.contains("[x] Bug B"));
    }

    #[test]
    fn render_review_output_text_with_explanation_and_findings() {
        let output = ReviewOutputEvent {
            findings: vec![make_finding("Bug", "Body.", "/f.rs", 1, 2)],
            overall_correctness: "good".to_string(),
            overall_explanation: "All good.".to_string(),
            overall_confidence_score: 0.8,
        };
        let text = render_review_output_text(&output);
        assert!(text.contains("All good."));
        assert!(text.contains("Bug"));
    }

    #[test]
    fn render_review_output_text_empty_returns_fallback() {
        let output = ReviewOutputEvent::default();
        let text = render_review_output_text(&output);
        assert_eq!(text, "Reviewer failed to output a response.");
    }

    #[test]
    fn render_review_output_text_only_explanation() {
        let output = ReviewOutputEvent {
            overall_explanation: "Just explanation.".to_string(),
            ..Default::default()
        };
        let text = render_review_output_text(&output);
        assert_eq!(text, "Just explanation.");
    }

    #[test]
    fn render_review_output_text_only_findings() {
        let output = ReviewOutputEvent {
            findings: vec![make_finding("Issue", "Detail.", "/x.rs", 5, 10)],
            ..Default::default()
        };
        let text = render_review_output_text(&output);
        assert!(!text.contains("Reviewer failed"));
        assert!(text.contains("Issue"));
    }

    #[test]
    fn multiline_body_indented() {
        let findings = vec![make_finding("Bug", "Line 1.\nLine 2.", "/f.rs", 1, 2)];
        let text = format_review_findings_block(&findings, None);
        assert!(text.contains("  Line 1."));
        assert!(text.contains("  Line 2."));
    }
}
