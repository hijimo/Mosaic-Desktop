//! Property 2: Protocol types use camelCase serialization.
//!
//! Validates Requirement 1.5: all protocol type JSON output uses camelCase field names only.
//! Uses recursive key inspection to verify no JSON key contains snake_case patterns.

#[cfg(test)]
mod tests {
    use crate::protocol::error::{CodexError, ErrorCode};
    use crate::protocol::event::*;
    use crate::protocol::submission::{Op, Submission};
    use crate::protocol::types::*;
    use proptest::prelude::*;

    // ── snake_case detection ─────────────────────────────────────────

    fn assert_no_snake_case_keys(json: &str) {
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        check_value_keys(&value);
    }

    fn check_value_keys(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    assert!(
                        !has_snake_case(key),
                        "Found snake_case key: \"{key}\" in JSON object"
                    );
                    check_value_keys(child);
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    check_value_keys(item);
                }
            }
            _ => {}
        }
    }

    fn has_snake_case(s: &str) -> bool {
        let bytes = s.as_bytes();
        for i in 1..bytes.len().saturating_sub(1) {
            if bytes[i] == b'_'
                && bytes[i - 1].is_ascii_lowercase()
                && bytes[i + 1].is_ascii_lowercase()
            {
                return true;
            }
        }
        false
    }

    // ── Leaf strategies ──────────────────────────────────────────────

    fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
        prop_oneof![
            any::<bool>().prop_map(serde_json::Value::Bool),
            any::<i64>().prop_map(|n| serde_json::Value::Number(n.into())),
            "[a-zA-Z0-9 _-]{0,30}".prop_map(serde_json::Value::String),
        ]
    }

    fn arb_json_object() -> impl Strategy<Value = serde_json::Value> {
        prop::collection::hash_map("[a-z]{1,8}", arb_json_value(), 0..3)
            .prop_map(|m| serde_json::Value::Object(m.into_iter().collect()))
    }

    fn arb_pathbuf() -> impl Strategy<Value = std::path::PathBuf> {
        "[a-zA-Z0-9/_-]{1,30}".prop_map(std::path::PathBuf::from)
    }

    fn arb_safe_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 _-]{0,30}"
    }

    // ── types.rs strategies ──────────────────────────────────────────

    fn arb_network_access() -> impl Strategy<Value = NetworkAccess> {
        prop_oneof![
            Just(NetworkAccess::Restricted),
            Just(NetworkAccess::Enabled)
        ]
    }

    fn arb_read_only_access() -> impl Strategy<Value = ReadOnlyAccess> {
        prop_oneof![
            (any::<bool>(), prop::collection::vec(arb_pathbuf(), 0..2)).prop_map(
                |(include_platform_defaults, readable_roots)| ReadOnlyAccess::Restricted {
                    include_platform_defaults,
                    readable_roots,
                }
            ),
            Just(ReadOnlyAccess::FullAccess),
        ]
    }

    fn arb_sandbox_policy() -> impl Strategy<Value = SandboxPolicy> {
        prop_oneof![
            Just(SandboxPolicy::DangerFullAccess),
            arb_read_only_access().prop_map(|access| SandboxPolicy::ReadOnly { access }),
            arb_network_access()
                .prop_map(|network_access| SandboxPolicy::ExternalSandbox { network_access }),
            (
                prop::collection::vec(arb_pathbuf(), 0..2),
                arb_read_only_access(),
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
            )
                .prop_map(
                    |(
                        writable_roots,
                        read_only_access,
                        network_access,
                        exclude_tmpdir_env_var,
                        exclude_slash_tmp,
                    )| {
                        SandboxPolicy::WorkspaceWrite {
                            writable_roots,
                            read_only_access,
                            network_access,
                            exclude_tmpdir_env_var,
                            exclude_slash_tmp,
                        }
                    }
                ),
        ]
    }

    fn arb_reject_config() -> impl Strategy<Value = RejectConfig> {
        (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
            |(sandbox_approval, rules, mcp_elicitations)| RejectConfig {
                sandbox_approval,
                rules,
                mcp_elicitations,
            },
        )
    }

    fn arb_ask_for_approval() -> impl Strategy<Value = AskForApproval> {
        prop_oneof![
            Just(AskForApproval::UnlessTrusted),
            Just(AskForApproval::OnFailure),
            Just(AskForApproval::OnRequest),
            arb_reject_config().prop_map(AskForApproval::Reject),
            Just(AskForApproval::Never),
        ]
    }

    fn arb_exec_policy_amendment() -> impl Strategy<Value = ExecPolicyAmendment> {
        prop::collection::vec(arb_safe_string(), 1..4)
            .prop_map(|command| ExecPolicyAmendment { command })
    }

    fn arb_network_policy_rule_action() -> impl Strategy<Value = NetworkPolicyRuleAction> {
        prop_oneof![
            Just(NetworkPolicyRuleAction::Allow),
            Just(NetworkPolicyRuleAction::Deny),
        ]
    }

    fn arb_network_policy_amendment() -> impl Strategy<Value = NetworkPolicyAmendment> {
        (arb_safe_string(), arb_network_policy_rule_action())
            .prop_map(|(host, action)| NetworkPolicyAmendment { host, action })
    }

    fn arb_review_decision() -> impl Strategy<Value = ReviewDecision> {
        prop_oneof![
            Just(ReviewDecision::Approved),
            arb_exec_policy_amendment().prop_map(|amendment| {
                ReviewDecision::ApprovedExecpolicyAmendment {
                    proposed_execpolicy_amendment: amendment,
                }
            }),
            Just(ReviewDecision::ApprovedForSession),
            arb_network_policy_amendment().prop_map(|amendment| {
                ReviewDecision::NetworkPolicyAmendment {
                    network_policy_amendment: amendment,
                }
            }),
            Just(ReviewDecision::Denied),
            Just(ReviewDecision::Abort),
        ]
    }

    fn arb_effort() -> impl Strategy<Value = Effort> {
        prop_oneof![Just(Effort::Low), Just(Effort::Medium), Just(Effort::High)]
    }

    fn arb_reasoning_summary() -> impl Strategy<Value = ReasoningSummary> {
        prop_oneof![
            Just(ReasoningSummary::Auto),
            Just(ReasoningSummary::Concise),
            Just(ReasoningSummary::Detailed),
            Just(ReasoningSummary::None),
        ]
    }

    fn arb_service_tier() -> impl Strategy<Value = ServiceTier> {
        Just(ServiceTier::Fast)
    }

    fn arb_mode_kind() -> impl Strategy<Value = ModeKind> {
        prop_oneof![Just(ModeKind::Plan), Just(ModeKind::Default)]
    }

    fn arb_collaboration_mode_settings() -> impl Strategy<Value = CollaborationModeSettings> {
        (
            arb_safe_string(),
            prop::option::of(arb_effort()),
            prop::option::of(arb_safe_string()),
        )
            .prop_map(|(model, reasoning_effort, developer_instructions)| {
                CollaborationModeSettings {
                    model,
                    reasoning_effort,
                    developer_instructions,
                }
            })
    }

    fn arb_collaboration_mode() -> impl Strategy<Value = CollaborationMode> {
        (arb_mode_kind(), arb_collaboration_mode_settings())
            .prop_map(|(mode, settings)| CollaborationMode { mode, settings })
    }

    fn arb_personality() -> impl Strategy<Value = Personality> {
        prop_oneof![
            Just(Personality::None),
            Just(Personality::Friendly),
            Just(Personality::Pragmatic),
        ]
    }

    fn arb_conversation_start_params() -> impl Strategy<Value = ConversationStartParams> {
        (arb_safe_string(), prop::option::of(arb_safe_string()))
            .prop_map(|(prompt, session_id)| ConversationStartParams { prompt, session_id })
    }

    fn arb_realtime_audio_frame() -> impl Strategy<Value = RealtimeAudioFrame> {
        (
            arb_safe_string(),
            any::<u32>(),
            any::<u16>(),
            prop::option::of(any::<u32>()),
        )
            .prop_map(|(data, sample_rate, num_channels, samples_per_channel)| {
                RealtimeAudioFrame {
                    data,
                    sample_rate,
                    num_channels,
                    samples_per_channel,
                }
            })
    }

    fn arb_conversation_audio_params() -> impl Strategy<Value = ConversationAudioParams> {
        arb_realtime_audio_frame().prop_map(|frame| ConversationAudioParams { frame })
    }

    fn arb_conversation_text_params() -> impl Strategy<Value = ConversationTextParams> {
        arb_safe_string().prop_map(|text| ConversationTextParams { text })
    }

    fn arb_user_input() -> impl Strategy<Value = UserInput> {
        prop_oneof![
            arb_safe_string().prop_map(|text| UserInput::Text { text }),
            arb_safe_string().prop_map(|image_url| UserInput::Image { image_url }),
            arb_pathbuf().prop_map(|path| UserInput::LocalImage { path }),
        ]
    }

    fn arb_content_item() -> impl Strategy<Value = ContentItem> {
        prop_oneof![
            arb_safe_string().prop_map(|text| ContentItem::Text { text }),
            arb_safe_string().prop_map(|url| ContentItem::Image { url }),
            prop::collection::vec(any::<u8>(), 0..16)
                .prop_map(|data| ContentItem::InputAudio { data }),
        ]
    }

    fn arb_content_or_items() -> impl Strategy<Value = ContentOrItems> {
        prop_oneof![
            arb_safe_string().prop_map(ContentOrItems::String),
            prop::collection::vec(arb_content_item(), 0..3).prop_map(ContentOrItems::Items),
        ]
    }

    fn arb_function_call_output_payload() -> impl Strategy<Value = FunctionCallOutputPayload> {
        arb_content_or_items().prop_map(|content| FunctionCallOutputPayload { content })
    }

    fn arb_response_input_item() -> impl Strategy<Value = ResponseInputItem> {
        prop_oneof![
            (arb_safe_string(), arb_safe_string())
                .prop_map(|(role, content)| ResponseInputItem::Message { role, content }),
            (arb_safe_string(), arb_safe_string(), arb_safe_string()).prop_map(
                |(call_id, name, arguments)| ResponseInputItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                }
            ),
            (arb_safe_string(), arb_function_call_output_payload()).prop_map(
                |(call_id, output)| ResponseInputItem::FunctionOutput { call_id, output }
            ),
        ]
    }

    fn arb_dynamic_tool_spec() -> impl Strategy<Value = DynamicToolSpec> {
        (arb_safe_string(), arb_safe_string(), arb_json_object()).prop_map(
            |(name, description, input_schema)| DynamicToolSpec {
                name,
                description,
                input_schema,
            },
        )
    }

    fn arb_dynamic_tool_call_request() -> impl Strategy<Value = DynamicToolCallRequest> {
        (
            arb_safe_string(),
            arb_safe_string(),
            arb_safe_string(),
            arb_json_object(),
        )
            .prop_map(
                |(call_id, turn_id, tool, arguments)| DynamicToolCallRequest {
                    call_id,
                    turn_id,
                    tool,
                    arguments,
                },
            )
    }

    fn arb_dynamic_tool_call_output_content_item(
    ) -> impl Strategy<Value = DynamicToolCallOutputContentItem> {
        prop_oneof![
            arb_safe_string().prop_map(|text| DynamicToolCallOutputContentItem::InputText { text }),
            arb_safe_string()
                .prop_map(|image_url| DynamicToolCallOutputContentItem::InputImage { image_url }),
        ]
    }

    fn arb_dynamic_tool_response() -> impl Strategy<Value = DynamicToolResponse> {
        (
            prop::collection::vec(arb_dynamic_tool_call_output_content_item(), 0..3),
            any::<bool>(),
        )
            .prop_map(|(content_items, success)| DynamicToolResponse {
                content_items,
                success,
            })
    }

    fn arb_file_change() -> impl Strategy<Value = FileChange> {
        prop_oneof![
            arb_safe_string().prop_map(|content| FileChange::Add { content }),
            arb_safe_string().prop_map(|content| FileChange::Delete { content }),
            (arb_safe_string(), prop::option::of(arb_pathbuf())).prop_map(
                |(unified_diff, move_path)| FileChange::Update {
                    unified_diff,
                    move_path,
                }
            ),
        ]
    }

    fn arb_mcp_invocation() -> impl Strategy<Value = McpInvocation> {
        (
            arb_safe_string(),
            arb_safe_string(),
            prop::option::of(arb_json_object()),
        )
            .prop_map(|(server, tool, arguments)| McpInvocation {
                server,
                tool,
                arguments,
            })
    }

    fn arb_mcp_server_refresh_config() -> impl Strategy<Value = McpServerRefreshConfig> {
        (arb_json_object(), arb_json_object()).prop_map(
            |(mcp_servers, mcp_oauth_credentials_store_mode)| McpServerRefreshConfig {
                mcp_servers,
                mcp_oauth_credentials_store_mode,
            },
        )
    }

    fn arb_parsed_command() -> impl Strategy<Value = ParsedCommand> {
        (
            arb_safe_string(),
            prop::collection::vec(arb_safe_string(), 0..3),
        )
            .prop_map(|(program, args)| ParsedCommand { program, args })
    }

    fn arb_exec_command_source() -> impl Strategy<Value = ExecCommandSource> {
        prop_oneof![
            Just(ExecCommandSource::Agent),
            Just(ExecCommandSource::UserShell),
        ]
    }

    fn arb_mcp_startup_status() -> impl Strategy<Value = McpStartupStatus> {
        prop_oneof![
            Just(McpStartupStatus::Starting),
            Just(McpStartupStatus::Ready),
            arb_safe_string().prop_map(|error| McpStartupStatus::Failed { error }),
            Just(McpStartupStatus::Cancelled),
        ]
    }

    fn arb_mcp_startup_failure() -> impl Strategy<Value = McpStartupFailure> {
        (arb_safe_string(), arb_safe_string())
            .prop_map(|(server, error)| McpStartupFailure { server, error })
    }

    fn arb_token_usage() -> impl Strategy<Value = TokenUsage> {
        (
            any::<i64>(),
            any::<i64>(),
            any::<i64>(),
            any::<i64>(),
            any::<i64>(),
        )
            .prop_map(
                |(
                    input_tokens,
                    cached_input_tokens,
                    output_tokens,
                    reasoning_output_tokens,
                    total_tokens,
                )| {
                    TokenUsage {
                        input_tokens,
                        cached_input_tokens,
                        output_tokens,
                        reasoning_output_tokens,
                        total_tokens,
                    }
                },
            )
    }

    fn arb_token_usage_info() -> impl Strategy<Value = TokenUsageInfo> {
        (
            arb_token_usage(),
            arb_token_usage(),
            prop::option::of(any::<i64>()),
        )
            .prop_map(
                |(total_token_usage, last_token_usage, model_context_window)| TokenUsageInfo {
                    total_token_usage,
                    last_token_usage,
                    model_context_window,
                },
            )
    }

    fn arb_turn_context_overrides() -> impl Strategy<Value = TurnContextOverrides> {
        (
            prop::option::of(arb_safe_string()),
            prop::option::of(arb_sandbox_policy()),
            prop::option::of(arb_ask_for_approval()),
            prop::option::of(arb_pathbuf()),
            prop::option::of(arb_collaboration_mode()),
            prop::option::of(arb_personality()),
        )
            .prop_map(
                |(model, sandbox_policy, approval_policy, cwd, collaboration_mode, personality)| {
                    TurnContextOverrides {
                        model,
                        sandbox_policy,
                        approval_policy,
                        cwd,
                        collaboration_mode,
                        personality,
                    }
                },
            )
    }

    fn arb_codex_error_info() -> impl Strategy<Value = CodexErrorInfo> {
        prop_oneof![
            Just(CodexErrorInfo::ContextWindowExceeded),
            Just(CodexErrorInfo::UsageLimitExceeded),
            Just(CodexErrorInfo::ServerOverloaded),
            prop::option::of(any::<u16>()).prop_map(|http_status_code| {
                CodexErrorInfo::HttpConnectionFailed { http_status_code }
            }),
            Just(CodexErrorInfo::InternalServerError),
            Just(CodexErrorInfo::Unauthorized),
            Just(CodexErrorInfo::BadRequest),
            Just(CodexErrorInfo::SandboxError),
            Just(CodexErrorInfo::ThreadRollbackFailed),
            Just(CodexErrorInfo::Other),
        ]
    }

    fn arb_elicitation_action() -> impl Strategy<Value = ElicitationAction> {
        prop_oneof![
            Just(ElicitationAction::Accept),
            Just(ElicitationAction::Decline),
            Just(ElicitationAction::Cancel),
        ]
    }

    fn arb_turn_abort_reason() -> impl Strategy<Value = TurnAbortReason> {
        prop_oneof![
            Just(TurnAbortReason::Interrupted),
            Just(TurnAbortReason::Replaced),
            Just(TurnAbortReason::ReviewEnded),
        ]
    }

    fn arb_error_code() -> impl Strategy<Value = ErrorCode> {
        prop_oneof![
            Just(ErrorCode::InvalidInput),
            Just(ErrorCode::ToolExecutionFailed),
            Just(ErrorCode::McpServerUnavailable),
            Just(ErrorCode::ConfigurationError),
            Just(ErrorCode::SandboxViolation),
            Just(ErrorCode::ApprovalDenied),
            Just(ErrorCode::SessionError),
            Just(ErrorCode::InternalError),
        ]
    }

    fn arb_codex_error() -> impl Strategy<Value = CodexError> {
        (
            arb_error_code(),
            "[a-zA-Z0-9 _.-]{0,100}",
            prop::option::of(arb_json_value()),
        )
            .prop_map(|(code, message, details)| CodexError {
                code,
                message,
                details,
            })
    }

    // ── Op strategy ──────────────────────────────────────────────────

    fn arb_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            // UserTurn
            (
                prop::collection::vec(arb_user_input(), 0..3),
                arb_pathbuf(),
                arb_ask_for_approval(),
                arb_sandbox_policy(),
                arb_safe_string(),
                prop::option::of(arb_effort()),
                prop::option::of(arb_reasoning_summary()),
                prop::option::of(arb_service_tier()),
                prop::option::of(arb_json_object()),
                prop::option::of(arb_collaboration_mode()),
                prop::option::of(arb_personality()),
            )
                .prop_map(
                    |(
                        items,
                        cwd,
                        approval_policy,
                        sandbox_policy,
                        model,
                        effort,
                        summary,
                        service_tier,
                        final_output_json_schema,
                        collaboration_mode,
                        personality,
                    )| {
                        Op::UserTurn {
                            items,
                            cwd,
                            approval_policy,
                            sandbox_policy,
                            model,
                            effort,
                            summary,
                            service_tier,
                            final_output_json_schema,
                            collaboration_mode,
                            personality,
                        }
                    }
                ),
            // UserInput
            (
                prop::collection::vec(arb_user_input(), 0..3),
                prop::option::of(arb_json_object()),
            )
                .prop_map(|(items, final_output_json_schema)| Op::UserInput {
                    items,
                    final_output_json_schema,
                }),
            (arb_safe_string(), arb_json_object())
                .prop_map(|(id, response)| Op::UserInputAnswer { id, response }),
            Just(Op::Interrupt),
            Just(Op::Shutdown),
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                arb_review_decision()
            )
                .prop_map(|(id, turn_id, decision)| Op::ExecApproval {
                    id,
                    turn_id,
                    decision
                }),
            (arb_safe_string(), arb_review_decision())
                .prop_map(|(id, decision)| Op::PatchApproval { id, decision }),
            (
                arb_safe_string(),
                arb_safe_string(),
                arb_elicitation_action()
            )
                .prop_map(|(server_name, request_id, decision)| {
                    Op::ResolveElicitation {
                        server_name,
                        request_id,
                        decision,
                    }
                }),
            (arb_safe_string(), arb_dynamic_tool_response())
                .prop_map(|(id, response)| Op::DynamicToolResponse { id, response }),
            arb_safe_string().prop_map(|text| Op::AddToHistory { text }),
            Just(Op::ListMcpTools),
            arb_mcp_server_refresh_config().prop_map(|config| Op::RefreshMcpServers { config }),
            Just(Op::ReloadUserConfig),
            (prop::collection::vec(arb_pathbuf(), 0..2), any::<bool>())
                .prop_map(|(cwds, force_reload)| Op::ListSkills { cwds, force_reload }),
            Just(Op::ListCustomPrompts),
            arb_conversation_start_params().prop_map(Op::RealtimeConversationStart),
            arb_conversation_audio_params().prop_map(Op::RealtimeConversationAudio),
            arb_conversation_text_params().prop_map(Op::RealtimeConversationText),
            Just(Op::RealtimeConversationClose),
            Just(Op::Compact),
            Just(Op::Undo),
            (1..100u32).prop_map(|num_turns| Op::ThreadRollback { num_turns }),
            arb_safe_string().prop_map(|name| Op::SetThreadName { name }),
            Just(Op::DropMemories),
            Just(Op::UpdateMemories),
            arb_safe_string().prop_map(|command| Op::RunUserShellCommand { command }),
            Just(Op::ListModels),
            Just(Op::CleanBackgroundTerminals),
        ]
    }

    fn arb_submission() -> impl Strategy<Value = Submission> {
        (arb_safe_string(), arb_op()).prop_map(|(id, op)| Submission { id, op })
    }

    // ── EventMsg strategy ────────────────────────────────────────────

    fn arb_event_msg() -> impl Strategy<Value = EventMsg> {
        prop_oneof![
            (arb_safe_string(), prop::option::of(arb_codex_error_info())).prop_map(
                |(message, codex_error_info)| EventMsg::Error(ErrorEvent {
                    message,
                    codex_error_info,
                })
            ),
            arb_safe_string().prop_map(|message| EventMsg::Warning(WarningEvent { message })),
            (
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string(),
                arb_ask_for_approval(),
                arb_sandbox_policy(),
                arb_pathbuf(),
                any::<u64>(),
                any::<usize>(),
            )
                .prop_map(
                    |(
                        session_id,
                        model,
                        model_provider_id,
                        approval_policy,
                        sandbox_policy,
                        cwd,
                        history_log_id,
                        history_entry_count,
                    )| {
                        EventMsg::SessionConfigured(SessionConfiguredEvent {
                            session_id,
                            model,
                            model_provider_id,
                            approval_policy,
                            sandbox_policy,
                            cwd,
                            history_log_id,
                            history_entry_count,
                        })
                    }
                ),
            (
                arb_safe_string(),
                prop::option::of(any::<i64>()),
                arb_mode_kind()
            )
                .prop_map(
                    |(turn_id, model_context_window, collaboration_mode_kind)| {
                        EventMsg::TurnStarted(TurnStartedEvent {
                            turn_id,
                            model_context_window,
                            collaboration_mode_kind,
                        })
                    }
                ),
            (arb_safe_string(), prop::option::of(arb_safe_string())).prop_map(
                |(turn_id, last_agent_message)| {
                    EventMsg::TurnComplete(TurnCompleteEvent {
                        turn_id,
                        last_agent_message,
                    })
                }
            ),
            (prop::option::of(arb_safe_string()), arb_turn_abort_reason()).prop_map(
                |(turn_id, reason)| { EventMsg::TurnAborted(TurnAbortedEvent { turn_id, reason }) }
            ),
            prop::option::of(arb_token_usage_info())
                .prop_map(|info| EventMsg::TokenCount(TokenCountEvent { info })),
            arb_safe_string()
                .prop_map(|message| EventMsg::AgentMessage(AgentMessageEvent { message })),
            arb_safe_string()
                .prop_map(|delta| EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta })),
            arb_safe_string().prop_map(|delta| EventMsg::AgentReasoningDelta(
                AgentReasoningDeltaEvent { delta }
            )),
            (
                arb_safe_string(),
                arb_safe_string(),
                prop::collection::vec(arb_safe_string(), 0..3),
                arb_pathbuf(),
                prop::collection::vec(arb_parsed_command(), 0..2),
                arb_exec_command_source(),
            )
                .prop_map(|(call_id, turn_id, command, cwd, parsed_cmd, source)| {
                    EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                        call_id,
                        turn_id,
                        command,
                        cwd,
                        parsed_cmd,
                        source,
                    })
                }),
            (arb_safe_string(), arb_safe_string()).prop_map(|(call_id, delta)| {
                EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent { call_id, delta })
            }),
            (arb_safe_string(), arb_mcp_invocation()).prop_map(|(call_id, invocation)| {
                EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
                    call_id,
                    invocation,
                })
            }),
            (arb_safe_string(), arb_mcp_invocation(), arb_json_object()).prop_map(
                |(call_id, invocation, result)| {
                    EventMsg::McpToolCallEnd(McpToolCallEndEvent {
                        call_id,
                        invocation,
                        result,
                    })
                }
            ),
            (arb_safe_string(), arb_mcp_startup_status()).prop_map(|(server, status)| {
                EventMsg::McpStartupUpdate(McpStartupUpdateEvent { server, status })
            }),
            (
                prop::collection::vec(arb_safe_string(), 0..3),
                prop::collection::vec(arb_mcp_startup_failure(), 0..2),
                prop::collection::vec(arb_safe_string(), 0..2),
            )
                .prop_map(|(ready, failed, cancelled)| {
                    EventMsg::McpStartupComplete(McpStartupCompleteEvent {
                        ready,
                        failed,
                        cancelled,
                    })
                }),
            Just(EventMsg::ContextCompacted(ContextCompactedEvent)),
            (1..100u32).prop_map(|num_turns| {
                EventMsg::ThreadRolledBack(ThreadRolledBackEvent { num_turns })
            }),
            arb_safe_string().prop_map(|message| {
                EventMsg::BackgroundEvent(BackgroundEventEvent { message })
            }),
            Just(EventMsg::ShutdownComplete),
        ]
    }

    fn arb_event() -> impl Strategy<Value = Event> {
        (arb_safe_string(), arb_event_msg()).prop_map(|(id, msg)| Event { id, msg })
    }

    // ── Property 2: camelCase serialization tests (100 iterations each) ──

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn submission_uses_camel_case(v in arb_submission()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn op_uses_camel_case(v in arb_op()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn event_uses_camel_case(v in arb_event()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn event_msg_uses_camel_case(v in arb_event_msg()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn sandbox_policy_uses_camel_case(v in arb_sandbox_policy()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn review_decision_uses_camel_case(v in arb_review_decision()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn content_item_uses_camel_case(v in arb_content_item()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn user_input_uses_camel_case(v in arb_user_input()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn response_input_item_uses_camel_case(v in arb_response_input_item()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn dynamic_tool_spec_uses_camel_case(v in arb_dynamic_tool_spec()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn dynamic_tool_call_request_uses_camel_case(v in arb_dynamic_tool_call_request()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn codex_error_uses_camel_case(v in arb_codex_error()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn personality_uses_camel_case(v in arb_personality()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn turn_context_overrides_uses_camel_case(v in arb_turn_context_overrides()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn token_usage_uses_camel_case(v in arb_token_usage()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn token_usage_info_uses_camel_case(v in arb_token_usage_info()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn file_change_uses_camel_case(v in arb_file_change()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn mcp_invocation_uses_camel_case(v in arb_mcp_invocation()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn collaboration_mode_uses_camel_case(v in arb_collaboration_mode()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }

        #[test]
        fn realtime_audio_frame_uses_camel_case(v in arb_realtime_audio_frame()) {
            let json = serde_json::to_string(&v).unwrap();
            assert_no_snake_case_keys(&json);
        }
    }
}
