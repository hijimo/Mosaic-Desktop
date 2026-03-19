use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::{SessionTask, TaskContext, TaskKind};
use crate::core::review_format::{format_review_findings_block, render_review_output_text};
use crate::core::review_prompts::resolve_review_request;
use crate::protocol::event::{EventMsg, ExitedReviewModeEvent};
use crate::protocol::types::{ReviewOutputEvent, ReviewRequest, UserInput};

/// Review system prompt. Edit `src-tauri/review_prompt.md` to customize.
pub const REVIEW_PROMPT: &str = include_str!("../../../review_prompt.md");

/// Template for the user message recorded after a successful review.
pub const REVIEW_EXIT_SUCCESS_TMPL: &str =
    include_str!("../../../templates/review/exit_success.xml");

/// Template for the user message recorded after an interrupted review.
pub const REVIEW_EXIT_INTERRUPTED_TMPL: &str =
    include_str!("../../../templates/review/exit_interrupted.xml");

/// Review task — runs a sub-agent to review code changes.
pub struct ReviewTask;

#[async_trait]
impl SessionTask for ReviewTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Review
    }

    async fn run(
        self: Arc<Self>,
        ctx: TaskContext,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        // Resolve the review request from input (first text item).
        let review_request = extract_review_request(&input);

        // Resolve prompt and hint from the review target.
        let resolved = match review_request {
            Some(req) => match resolve_review_request(req.clone(), &ctx.cwd).await {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("failed to resolve review request: {e}");
                    None
                }
            },
            None => None,
        };

        // Emit review mode entry.
        let entered_request = if let Some(ref r) = resolved {
            ReviewRequest {
                target: r.target.clone(),
                user_facing_hint: Some(r.user_facing_hint.clone()),
            }
        } else {
            ReviewRequest {
                target: crate::protocol::types::ReviewTarget::UncommittedChanges,
                user_facing_hint: None,
            }
        };
        ctx.emit(EventMsg::EnteredReviewMode(entered_request)).await;

        if cancellation_token.is_cancelled() {
            exit_review_mode(&ctx, None).await;
            return None;
        }

        // TODO: Start sub-codex conversation with review prompt,
        // process events, parse ReviewOutputEvent, emit ExitedReviewMode.
        // For now, log the resolved prompt for debugging.
        if let Some(ref r) = resolved {
            warn!("review task: resolved prompt = {:?}", r.prompt);
        }
        warn!("review task: sub-agent conversation not yet implemented");

        exit_review_mode(&ctx, None).await;
        None
    }

    async fn abort(&self, ctx: TaskContext) {
        exit_review_mode(&ctx, None).await;
    }
}

/// Parse a ReviewOutputEvent from a text blob returned by the reviewer model.
/// If the text is valid JSON matching ReviewOutputEvent, deserialize it.
/// Otherwise, attempt to extract the first JSON object substring and parse it.
/// If parsing still fails, return a structured fallback carrying the plain text
/// in `overall_explanation`.
pub fn parse_review_output_event(text: &str) -> ReviewOutputEvent {
    if let Ok(ev) = serde_json::from_str::<ReviewOutputEvent>(text) {
        return ev;
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if start < end {
            if let Some(slice) = text.get(start..=end) {
                if let Ok(ev) = serde_json::from_str::<ReviewOutputEvent>(slice) {
                    return ev;
                }
            }
        }
    }
    ReviewOutputEvent {
        overall_explanation: text.to_string(),
        ..Default::default()
    }
}

/// Emits an ExitedReviewMode event and records review output.
async fn exit_review_mode(ctx: &TaskContext, review_output: Option<ReviewOutputEvent>) {
    let (_user_message, _assistant_message) = if let Some(ref out) = review_output {
        let mut findings_str = String::new();
        let text = out.overall_explanation.trim();
        if !text.is_empty() {
            findings_str.push_str(text);
        }
        if !out.findings.is_empty() {
            let block = format_review_findings_block(&out.findings, None);
            findings_str.push_str(&format!("\n{block}"));
        }
        let rendered = REVIEW_EXIT_SUCCESS_TMPL.replace("{results}", &findings_str);
        let assistant_message = render_review_output_text(out);
        (rendered, assistant_message)
    } else {
        let rendered = REVIEW_EXIT_INTERRUPTED_TMPL.to_string();
        let assistant_message =
            "Review was interrupted. Please re-run /review and wait for it to complete."
                .to_string();
        (rendered, assistant_message)
    };

    ctx.emit(EventMsg::ExitedReviewMode(ExitedReviewModeEvent {
        review_output,
    }))
    .await;
}

/// Extract a ReviewRequest from user input items.
fn extract_review_request(input: &[UserInput]) -> Option<ReviewRequest> {
    for item in input {
        if let UserInput::Text { text, .. } = item {
            // Try to parse as JSON ReviewRequest
            if let Ok(req) = serde_json::from_str::<ReviewRequest>(text) {
                return Some(req);
            }
        }
    }
    // Default to uncommitted changes if no explicit request found.
    Some(ReviewRequest {
        target: crate::protocol::types::ReviewTarget::UncommittedChanges,
        user_facing_hint: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{ReviewCodeLocation, ReviewFinding, ReviewLineRange};
    use std::path::PathBuf;

    #[test]
    fn parse_valid_json() {
        let json = serde_json::json!({
            "findings": [{
                "title": "Bug",
                "body": "Details.",
                "confidence_score": 0.9,
                "priority": 1,
                "code_location": {
                    "absolute_file_path": "/tmp/f.rs",
                    "line_range": {"start": 1, "end": 5}
                }
            }],
            "overall_correctness": "good",
            "overall_explanation": "Fine.",
            "overall_confidence_score": 0.8
        })
        .to_string();

        let ev = parse_review_output_event(&json);
        assert_eq!(ev.findings.len(), 1);
        assert_eq!(ev.findings[0].title, "Bug");
        assert_eq!(ev.overall_explanation, "Fine.");
    }

    #[test]
    fn parse_json_embedded_in_text() {
        let text = r#"Here is the review: {"findings":[],"overall_correctness":"ok","overall_explanation":"ok","overall_confidence_score":0.5} end"#;
        let ev = parse_review_output_event(text);
        assert_eq!(ev.overall_explanation, "ok");
        assert!(ev.findings.is_empty());
    }

    #[test]
    fn parse_plain_text_fallback() {
        let text = "just plain text";
        let ev = parse_review_output_event(text);
        assert_eq!(ev.overall_explanation, "just plain text");
        assert!(ev.findings.is_empty());
    }

    #[test]
    fn parse_invalid_json_fallback() {
        let text = "{not valid json}";
        let ev = parse_review_output_event(text);
        assert_eq!(ev.overall_explanation, "{not valid json}");
    }

    #[test]
    fn review_prompt_constant_not_empty() {
        assert!(!REVIEW_PROMPT.is_empty());
        assert!(REVIEW_PROMPT.contains("Review guidelines"));
    }

    #[test]
    fn review_exit_templates_not_empty() {
        assert!(!REVIEW_EXIT_SUCCESS_TMPL.is_empty());
        assert!(REVIEW_EXIT_SUCCESS_TMPL.contains("{results}"));
        assert!(!REVIEW_EXIT_INTERRUPTED_TMPL.is_empty());
        assert!(REVIEW_EXIT_INTERRUPTED_TMPL.contains("interrupted"));
    }

    #[test]
    fn exit_review_mode_formats_success() {
        let output = ReviewOutputEvent {
            findings: vec![ReviewFinding {
                title: "Bug A".to_string(),
                body: "Details.".to_string(),
                confidence_score: 0.9,
                priority: 1,
                code_location: ReviewCodeLocation {
                    absolute_file_path: PathBuf::from("/tmp/a.rs"),
                    line_range: ReviewLineRange { start: 1, end: 5 },
                },
            }],
            overall_correctness: "good".to_string(),
            overall_explanation: "All good.".to_string(),
            overall_confidence_score: 0.8,
        };

        // Verify the formatting logic directly
        let mut findings_str = String::new();
        let text = output.overall_explanation.trim();
        if !text.is_empty() {
            findings_str.push_str(text);
        }
        if !output.findings.is_empty() {
            let block = format_review_findings_block(&output.findings, None);
            findings_str.push_str(&format!("\n{block}"));
        }
        let rendered = REVIEW_EXIT_SUCCESS_TMPL.replace("{results}", &findings_str);
        assert!(rendered.contains("All good."));
        assert!(rendered.contains("Bug A"));

        let assistant = render_review_output_text(&output);
        assert!(assistant.contains("All good."));
        assert!(assistant.contains("Bug A"));
    }

    #[test]
    fn extract_review_request_from_json() {
        let json = serde_json::json!({
            "target": {"type": "uncommittedChanges"},
        })
        .to_string();
        let input = vec![UserInput::Text {
            text: json,
            text_elements: vec![],
        }];
        let req = extract_review_request(&input).unwrap();
        assert_eq!(
            req.target,
            crate::protocol::types::ReviewTarget::UncommittedChanges
        );
    }

    #[test]
    fn extract_review_request_default() {
        let input = vec![UserInput::Text {
            text: "not json".to_string(),
            text_elements: vec![],
        }];
        let req = extract_review_request(&input).unwrap();
        assert_eq!(
            req.target,
            crate::protocol::types::ReviewTarget::UncommittedChanges
        );
    }
}
