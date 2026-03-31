//! Built-in collaboration mode presets.

use crate::protocol::types::{
    CollaborationModeMask, Effort, ModeKind, TUI_VISIBLE_COLLABORATION_MODES,
};

const COLLABORATION_MODE_PLAN: &str = include_str!("../../../templates/collaboration_mode/plan.md");
const COLLABORATION_MODE_DEFAULT: &str =
    include_str!("../../../templates/collaboration_mode/default.md");
const KNOWN_MODE_NAMES_PLACEHOLDER: &str = "{{KNOWN_MODE_NAMES}}";
const REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER: &str = "{{REQUEST_USER_INPUT_AVAILABILITY}}";
const ASKING_QUESTIONS_GUIDANCE_PLACEHOLDER: &str = "{{ASKING_QUESTIONS_GUIDANCE}}";

/// Feature flags that control collaboration-mode behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CollaborationModesConfig {
    /// Enables `request_user_input` availability in Default mode.
    pub default_mode_request_user_input: bool,
}

/// Return the built-in collaboration mode presets (Plan + Default).
pub fn builtin_collaboration_mode_presets(
    config: CollaborationModesConfig,
) -> Vec<CollaborationModeMask> {
    vec![plan_preset(), default_preset(config)]
}

fn plan_preset() -> CollaborationModeMask {
    CollaborationModeMask {
        name: ModeKind::Plan.display_name().to_string(),
        mode: Some(ModeKind::Plan),
        model: None,
        reasoning_effort: Some(Some(Effort::Medium)),
        developer_instructions: Some(Some(COLLABORATION_MODE_PLAN.to_string())),
    }
}

fn default_preset(config: CollaborationModesConfig) -> CollaborationModeMask {
    CollaborationModeMask {
        name: ModeKind::Default.display_name().to_string(),
        mode: Some(ModeKind::Default),
        model: None,
        reasoning_effort: None,
        developer_instructions: Some(Some(default_mode_instructions(config))),
    }
}

fn default_mode_instructions(config: CollaborationModesConfig) -> String {
    let known_mode_names = format_mode_names(&TUI_VISIBLE_COLLABORATION_MODES);
    let request_user_input_availability = request_user_input_availability_message(
        ModeKind::Default,
        config.default_mode_request_user_input,
    );
    let asking_questions_guidance =
        asking_questions_guidance_message(config.default_mode_request_user_input);
    COLLABORATION_MODE_DEFAULT
        .replace(KNOWN_MODE_NAMES_PLACEHOLDER, &known_mode_names)
        .replace(
            REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER,
            &request_user_input_availability,
        )
        .replace(
            ASKING_QUESTIONS_GUIDANCE_PLACEHOLDER,
            &asking_questions_guidance,
        )
}

fn format_mode_names(modes: &[ModeKind]) -> String {
    let mode_names: Vec<&str> = modes.iter().map(|mode| mode.display_name()).collect();
    match mode_names.as_slice() {
        [] => "none".to_string(),
        [mode_name] => (*mode_name).to_string(),
        [first, second] => format!("{first} and {second}"),
        [..] => mode_names.join(", "),
    }
}

fn request_user_input_availability_message(
    mode: ModeKind,
    default_mode_request_user_input: bool,
) -> String {
    let mode_name = mode.display_name();
    if mode.allows_request_user_input()
        || (default_mode_request_user_input && mode == ModeKind::Default)
    {
        format!("The `request_user_input` tool is available in {mode_name} mode.")
    } else {
        format!(
            "The `request_user_input` tool is unavailable in {mode_name} mode. If you call it while in {mode_name} mode, it will return an error."
        )
    }
}

fn asking_questions_guidance_message(default_mode_request_user_input: bool) -> String {
    if default_mode_request_user_input {
        "In Default mode, strongly prefer making reasonable assumptions and executing the user's request rather than stopping to ask questions. If you absolutely must ask a question because the answer cannot be discovered from local context and a reasonable assumption would be risky, prefer using the `request_user_input` tool rather than writing a multiple choice question as a textual assistant message. Never write a multiple choice question as a textual assistant message.".to_string()
    } else {
        "In Default mode, strongly prefer making reasonable assumptions and executing the user's request rather than stopping to ask questions. If you absolutely must ask a question because the answer cannot be discovered from local context and a reasonable assumption would be risky, ask the user directly with a concise plain-text question. Never write a multiple choice question as a textual assistant message.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_use_mode_display_names() {
        assert_eq!(plan_preset().name, ModeKind::Plan.display_name());
        assert_eq!(
            default_preset(CollaborationModesConfig::default()).name,
            ModeKind::Default.display_name()
        );
        assert_eq!(plan_preset().reasoning_effort, Some(Some(Effort::Medium)));
    }

    #[test]
    fn default_mode_instructions_replace_all_placeholders() {
        let default_instructions = default_preset(CollaborationModesConfig {
            default_mode_request_user_input: true,
        })
        .developer_instructions
        .expect("default preset should include instructions")
        .expect("default instructions should be set");

        assert!(!default_instructions.contains(KNOWN_MODE_NAMES_PLACEHOLDER));
        assert!(!default_instructions.contains(REQUEST_USER_INPUT_AVAILABILITY_PLACEHOLDER));
        assert!(!default_instructions.contains(ASKING_QUESTIONS_GUIDANCE_PLACEHOLDER));

        let known_mode_names = format_mode_names(&TUI_VISIBLE_COLLABORATION_MODES);
        let expected_snippet = format!("Known mode names are {known_mode_names}.");
        assert!(default_instructions.contains(&expected_snippet));

        let expected_availability_message =
            request_user_input_availability_message(ModeKind::Default, true);
        assert!(default_instructions.contains(&expected_availability_message));
        assert!(default_instructions.contains("prefer using the `request_user_input` tool"));
    }

    #[test]
    fn default_mode_instructions_use_plain_text_questions_when_feature_disabled() {
        let default_instructions = default_preset(CollaborationModesConfig::default())
            .developer_instructions
            .expect("default preset should include instructions")
            .expect("default instructions should be set");

        assert!(!default_instructions.contains("prefer using the `request_user_input` tool"));
        assert!(default_instructions
            .contains("ask the user directly with a concise plain-text question"));
    }

    #[test]
    fn builtin_presets_returns_two_modes() {
        let presets = builtin_collaboration_mode_presets(CollaborationModesConfig::default());
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].name, "Plan");
        assert_eq!(presets[1].name, "Default");
    }

    #[test]
    fn plan_preset_has_medium_effort() {
        let preset = plan_preset();
        assert_eq!(preset.reasoning_effort, Some(Some(Effort::Medium)));
        assert!(preset
            .developer_instructions
            .unwrap()
            .unwrap()
            .contains("Plan Mode"));
    }

    #[test]
    fn format_mode_names_handles_edge_cases() {
        assert_eq!(format_mode_names(&[]), "none");
        assert_eq!(format_mode_names(&[ModeKind::Plan]), "Plan");
        assert_eq!(
            format_mode_names(&[ModeKind::Default, ModeKind::Plan]),
            "Default and Plan"
        );
        assert_eq!(
            format_mode_names(&[ModeKind::Default, ModeKind::Plan, ModeKind::Execute]),
            "Default, Plan, Execute"
        );
    }

    #[test]
    fn request_user_input_availability_plan_mode() {
        let msg = request_user_input_availability_message(ModeKind::Plan, false);
        assert!(msg.contains("available in Plan mode"));
    }

    #[test]
    fn request_user_input_availability_default_mode_disabled() {
        let msg = request_user_input_availability_message(ModeKind::Default, false);
        assert!(msg.contains("unavailable in Default mode"));
    }

    #[test]
    fn request_user_input_availability_default_mode_enabled() {
        let msg = request_user_input_availability_message(ModeKind::Default, true);
        assert!(msg.contains("available in Default mode"));
    }
}
