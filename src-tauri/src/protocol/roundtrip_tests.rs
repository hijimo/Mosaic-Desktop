#[cfg(test)]
mod tests {
    use crate::protocol::event::*;
    use crate::protocol::submission::{Op, Submission};
    use crate::protocol::types::*;
    use proptest::prelude::*;
    #[allow(unused_imports)]
    use std::path::PathBuf;
    use std::time::Duration;

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

    fn arb_duration() -> impl Strategy<Value = Duration> {
        any::<u64>().prop_map(Duration::from_millis)
    }

    fn arb_exec_output_stream() -> impl Strategy<Value = ExecOutputStream> {
        prop_oneof![
            Just(ExecOutputStream::Stdout),
            Just(ExecOutputStream::Stderr),
        ]
    }

    fn arb_call_tool_result() -> impl Strategy<Value = Result<CallToolResult, String>> {
        prop_oneof![
            (
                prop::option::of(arb_json_object()),
                prop::option::of(any::<bool>())
            )
                .prop_map(|(content, is_error)| Ok(CallToolResult { content, is_error })),
            arb_safe_string().prop_map(Err),
        ]
    }

    #[allow(dead_code)]
    fn arb_network_approval_context() -> impl Strategy<Value = NetworkApprovalContext> {
        (arb_safe_string(), arb_network_approval_protocol())
            .prop_map(|(host, protocol)| NetworkApprovalContext { host, protocol })
    }

    #[allow(dead_code)]
    fn arb_network_approval_protocol() -> impl Strategy<Value = NetworkApprovalProtocol> {
        prop_oneof![
            Just(NetworkApprovalProtocol::Http),
            Just(NetworkApprovalProtocol::Https),
            Just(NetworkApprovalProtocol::Socks5Tcp),
            Just(NetworkApprovalProtocol::Socks5Udp),
        ]
    }

    // ── types.rs strategies ──────────────────────────────────────────

    fn arb_network_access() -> impl Strategy<Value = NetworkAccess> {
        prop_oneof![
            Just(NetworkAccess::Restricted),
            Just(NetworkAccess::Enabled),
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
            (
                arb_safe_string(),
                prop::collection::vec(arb_text_element(), 0..2)
            )
                .prop_map(|(text, text_elements)| UserInput::Text {
                    text,
                    text_elements
                }),
            arb_safe_string().prop_map(|image_url| UserInput::Image { image_url }),
            arb_pathbuf().prop_map(|path| UserInput::LocalImage { path }),
            (arb_safe_string(), arb_pathbuf())
                .prop_map(|(name, path)| UserInput::Skill { name, path }),
            (arb_safe_string(), arb_safe_string())
                .prop_map(|(name, path)| UserInput::Mention { name, path }),
        ]
    }

    fn arb_text_element() -> impl Strategy<Value = TextElement> {
        (
            any::<usize>(),
            any::<usize>(),
            prop::option::of(arb_safe_string()),
        )
            .prop_map(|(start, end, placeholder)| TextElement {
                byte_range: ByteRange { start, end },
                placeholder,
            })
    }

    fn arb_content_item() -> impl Strategy<Value = ContentItem> {
        prop_oneof![
            arb_safe_string().prop_map(|text| ContentItem::InputText { text }),
            arb_safe_string().prop_map(|image_url| ContentItem::InputImage { image_url }),
            arb_safe_string().prop_map(|text| ContentItem::OutputText { text }),
        ]
    }

    /// Safe string that cannot be confused with a JSON array during
    /// untagged deserialization of `ContentOrItems`.
    fn arb_content_or_items() -> impl Strategy<Value = ContentOrItems> {
        prop_oneof![
            arb_safe_string().prop_map(FunctionCallOutputBody::Text),
            prop::collection::vec(arb_function_call_output_content_item(), 0..3)
                .prop_map(FunctionCallOutputBody::ContentItems),
        ]
    }

    fn arb_function_call_output_content_item(
    ) -> impl Strategy<Value = crate::protocol::types::FunctionCallOutputContentItem> {
        prop_oneof![
            arb_safe_string().prop_map(|text| {
                crate::protocol::types::FunctionCallOutputContentItem::InputText { text }
            }),
            arb_safe_string().prop_map(|image_url| {
                crate::protocol::types::FunctionCallOutputContentItem::InputImage { image_url }
            }),
        ]
    }

    fn arb_function_call_output_payload() -> impl Strategy<Value = FunctionCallOutputPayload> {
        arb_content_or_items().prop_map(|body| FunctionCallOutputPayload { body, success: None })
    }

    fn arb_response_input_item() -> impl Strategy<Value = ResponseInputItem> {
        prop_oneof![
            (arb_safe_string(), arb_safe_string())
                .prop_map(|(role, content)| ResponseInputItem::text_message(&role, content)),
            (arb_safe_string(), arb_safe_string(), arb_safe_string()).prop_map(
                |(call_id, name, arguments)| ResponseInputItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                }
            ),
            (arb_safe_string(), arb_function_call_output_payload()).prop_map(
                |(call_id, output)| ResponseInputItem::FunctionCallOutput { call_id, output }
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
        prop_oneof![
            (arb_safe_string(), arb_safe_string(), arb_pathbuf())
                .prop_map(|(cmd, name, path)| ParsedCommand::Read { cmd, name, path }),
            (arb_safe_string(), prop::option::of(arb_safe_string()))
                .prop_map(|(cmd, path)| ParsedCommand::ListFiles { cmd, path }),
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                prop::option::of(arb_safe_string())
            )
                .prop_map(|(cmd, query, path)| ParsedCommand::Search {
                    cmd,
                    query,
                    path
                }),
            arb_safe_string().prop_map(|cmd| ParsedCommand::Unknown { cmd }),
        ]
    }

    fn arb_exec_command_source() -> impl Strategy<Value = ExecCommandSource> {
        prop_oneof![
            Just(ExecCommandSource::Agent),
            Just(ExecCommandSource::UserShell),
        ]
    }

    #[allow(dead_code)]
    fn arb_exec_command_status() -> impl Strategy<Value = ExecCommandStatus> {
        prop_oneof![
            Just(ExecCommandStatus::Completed),
            Just(ExecCommandStatus::Failed),
            Just(ExecCommandStatus::Declined),
        ]
    }

    fn arb_patch_apply_status() -> impl Strategy<Value = PatchApplyStatus> {
        prop_oneof![
            Just(PatchApplyStatus::Completed),
            Just(PatchApplyStatus::Failed),
            Just(PatchApplyStatus::Declined),
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
            // UserInputAnswer
            (arb_safe_string(), arb_json_object())
                .prop_map(|(id, response)| Op::UserInputAnswer { id, response }),
            Just(Op::Interrupt),
            Just(Op::Shutdown),
            // ExecApproval
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                arb_review_decision(),
                prop::option::of(arb_safe_string())
            )
                .prop_map(|(id, turn_id, decision, custom_instructions)| {
                    Op::ExecApproval {
                        id,
                        turn_id,
                        decision,
                        custom_instructions,
                    }
                }),
            // PatchApproval
            (
                arb_safe_string(),
                arb_review_decision(),
                prop::option::of(arb_safe_string())
            )
                .prop_map(|(id, decision, custom_instructions)| Op::PatchApproval {
                    id,
                    decision,
                    custom_instructions
                }),
            // ResolveElicitation
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
            // DynamicToolResponse
            (arb_safe_string(), arb_dynamic_tool_response())
                .prop_map(|(id, response)| Op::DynamicToolResponse { id, response }),
            // AddToHistory
            arb_safe_string().prop_map(|text| Op::AddToHistory { text }),
            Just(Op::ListMcpTools),
            arb_mcp_server_refresh_config().prop_map(|config| Op::RefreshMcpServers { config }),
            Just(Op::ReloadUserConfig),
            // ListSkills
            (prop::collection::vec(arb_pathbuf(), 0..2), any::<bool>(),)
                .prop_map(|(cwds, force_reload)| Op::ListSkills { cwds, force_reload }),
            Just(Op::ListCustomPrompts),
            // Realtime
            arb_conversation_start_params().prop_map(Op::RealtimeConversationStart),
            arb_conversation_audio_params().prop_map(Op::RealtimeConversationAudio),
            arb_conversation_text_params().prop_map(Op::RealtimeConversationText),
            Just(Op::RealtimeConversationClose),
            // Context management
            Just(Op::Compact),
            Just(Op::Undo),
            (1..100u32).prop_map(|num_turns| Op::ThreadRollback { num_turns }),
            // Misc
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
            // Error
            (arb_safe_string(), prop::option::of(arb_codex_error_info())).prop_map(
                |(message, codex_error_info)| EventMsg::Error(ErrorEvent {
                    message,
                    codex_error_info,
                })
            ),
            // Warning
            arb_safe_string().prop_map(|message| EventMsg::Warning(WarningEvent { message })),
            // SessionConfigured
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                prop::option::of(arb_safe_string()),
                arb_safe_string(),
                arb_safe_string(),
                prop::option::of(arb_ask_for_approval()),
                prop::option::of(arb_sandbox_policy()),
                arb_pathbuf(),
                any::<u64>(),
                any::<usize>(),
            )
                .prop_map(
                    |(
                        session_id,
                        forked_from_id,
                        thread_name,
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
                            forked_from_id,
                            thread_name,
                            model,
                            model_provider_id,
                            approval_policy,
                            sandbox_policy,
                            cwd,
                            history_log_id,
                            history_entry_count,
                            mode: ModeKind::Default,
                            reasoning_effort: None,
                            reasoning_summary: None,
                            can_append: false,
                        })
                    }
                ),
            // TurnStarted
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
            // TurnComplete
            (arb_safe_string(), prop::option::of(arb_safe_string())).prop_map(
                |(turn_id, last_agent_message)| {
                    EventMsg::TurnComplete(TurnCompleteEvent {
                        turn_id,
                        last_agent_message,
                    })
                }
            ),
            // TurnAborted
            (prop::option::of(arb_safe_string()), arb_turn_abort_reason()).prop_map(
                |(turn_id, reason)| { EventMsg::TurnAborted(TurnAbortedEvent { turn_id, reason }) }
            ),
            // TokenCount
            prop::option::of(arb_token_usage_info())
                .prop_map(|info| EventMsg::TokenCount(TokenCountEvent { info, rate_limits: None })),
            // AgentMessage
            arb_safe_string()
                .prop_map(|message| EventMsg::AgentMessage(AgentMessageEvent { message })),
            // AgentMessageDelta
            arb_safe_string()
                .prop_map(|delta| EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta })),
            // AgentReasoningDelta
            arb_safe_string().prop_map(|delta| {
                EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta })
            }),
            // ExecCommandBegin
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                arb_safe_string(),
                prop::collection::vec(arb_safe_string(), 0..3),
                arb_pathbuf(),
                prop::collection::vec(arb_parsed_command(), 0..2),
                arb_exec_command_source(),
                prop::option::of(arb_safe_string()),
            )
                .prop_map(
                    |(
                        call_id,
                        process_id,
                        turn_id,
                        command,
                        cwd,
                        parsed_cmd,
                        source,
                        interaction_input,
                    )| {
                        EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                            call_id,
                            process_id,
                            turn_id,
                            command,
                            cwd,
                            parsed_cmd,
                            source,
                            interaction_input,
                        })
                    }
                ),
            // ExecCommandEnd
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                arb_safe_string(),
                prop::collection::vec(arb_safe_string(), 0..3),
                arb_pathbuf(),
                prop::collection::vec(arb_parsed_command(), 0..2),
                arb_exec_command_source(),
                prop::option::of(arb_safe_string()),
                arb_safe_string(),
                arb_safe_string(),
            )
                .prop_map(
                    |(
                        call_id,
                        process_id,
                        turn_id,
                        command,
                        cwd,
                        parsed_cmd,
                        source,
                        interaction_input,
                        stdout,
                        stderr,
                    )| {
                        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                            call_id,
                            process_id,
                            turn_id,
                            command,
                            cwd,
                            parsed_cmd,
                            source,
                            interaction_input,
                            stdout,
                            stderr,
                            aggregated_output: "aggregated".to_string(),
                            exit_code: 0,
                            duration: Duration::from_millis(100),
                            formatted_output: "formatted".to_string(),
                            status: ExecCommandStatus::Completed,
                        })
                    }
                ),
            // ExecCommandOutputDelta
            (
                arb_safe_string(),
                arb_safe_string(),
                prop::option::of(arb_exec_output_stream())
            )
                .prop_map(|(call_id, delta, stream)| {
                    EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
                        call_id,
                        delta,
                        stream,
                    })
                }),
            // McpToolCallBegin
            (arb_safe_string(), arb_mcp_invocation()).prop_map(|(call_id, invocation)| {
                EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
                    call_id,
                    invocation,
                })
            }),
            // McpToolCallEnd
            (
                arb_safe_string(),
                arb_mcp_invocation(),
                arb_duration(),
                arb_call_tool_result()
            )
                .prop_map(|(call_id, invocation, duration, result)| {
                    EventMsg::McpToolCallEnd(McpToolCallEndEvent {
                        call_id,
                        invocation,
                        duration,
                        result,
                    })
                }),
            // McpStartupUpdate
            (arb_safe_string(), arb_mcp_startup_status()).prop_map(|(server, status)| {
                EventMsg::McpStartupUpdate(McpStartupUpdateEvent { server, status })
            }),
            // McpStartupComplete
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
            // ContextCompacted
            Just(EventMsg::ContextCompacted(ContextCompactedEvent)),
            // ThreadRolledBack
            (1..100u32).prop_map(|num_turns| {
                EventMsg::ThreadRolledBack(ThreadRolledBackEvent { num_turns })
            }),
            // BackgroundEvent
            arb_safe_string().prop_map(|message| {
                EventMsg::BackgroundEvent(BackgroundEventEvent { message })
            }),
            // ShutdownComplete
            Just(EventMsg::ShutdownComplete),
        ]
    }

    fn arb_event() -> impl Strategy<Value = Event> {
        (arb_safe_string(), arb_event_msg()).prop_map(|(id, msg)| Event { id, msg })
    }

    // ── OverrideTurnContext strategy ─────────────────────────────────

    fn arb_override_turn_context_op() -> impl Strategy<Value = Op> {
        (
            prop::option::of(arb_pathbuf()),
            prop::option::of(arb_ask_for_approval()),
            prop::option::of(arb_sandbox_policy()),
            prop::option::of(arb_safe_string()),
            prop::option::of(arb_effort()),
            prop::option::of(arb_reasoning_summary()),
            prop::option::of(arb_service_tier()),
            prop::option::of(arb_collaboration_mode()),
            prop::option::of(arb_personality()),
        )
            .prop_map(
                |(
                    cwd,
                    approval_policy,
                    sandbox_policy,
                    model,
                    effort,
                    summary,
                    service_tier,
                    collaboration_mode,
                    personality,
                )| {
                    Op::OverrideTurnContext {
                        cwd,
                        approval_policy,
                        sandbox_policy,
                        model,
                        effort,
                        summary,
                        service_tier,
                        collaboration_mode,
                        personality,
                    }
                },
            )
    }

    // ── Additional event strategies for full coverage ────────────────

    fn arb_event_msg_extended() -> impl Strategy<Value = EventMsg> {
        prop_oneof![
            // ThreadNameUpdated
            (arb_safe_string(), prop::option::of(arb_safe_string())).prop_map(
                |(thread_id, thread_name)| {
                    EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
                        thread_id,
                        thread_name,
                    })
                }
            ),
            // PlanDelta
            (
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string()
            )
                .prop_map(|(thread_id, turn_id, item_id, delta)| {
                    EventMsg::PlanDelta(PlanDeltaEvent {
                        thread_id,
                        turn_id,
                        item_id,
                        delta,
                    })
                }),
            // ItemStarted
            (arb_safe_string(), arb_safe_string(), arb_json_object()).prop_map(
                |(thread_id, turn_id, item)| {
                    EventMsg::ItemStarted(ItemStartedEvent {
                        thread_id,
                        turn_id,
                        item,
                    })
                }
            ),
            // ItemCompleted
            (arb_safe_string(), arb_safe_string(), arb_json_object()).prop_map(
                |(thread_id, turn_id, item)| {
                    EventMsg::ItemCompleted(ItemCompletedEvent {
                        thread_id,
                        turn_id,
                        item,
                    })
                }
            ),
            // RawResponseItem
            arb_json_object()
                .prop_map(|item| EventMsg::RawResponseItem(RawResponseItemEvent { item })),
            // ExecApprovalRequest
            (
                arb_safe_string(),
                prop::option::of(arb_safe_string()),
                arb_safe_string(),
                prop::collection::vec(arb_safe_string(), 0..3),
                arb_pathbuf(),
                prop::option::of(arb_safe_string()),
                prop::collection::vec(arb_parsed_command(), 0..2),
            )
                .prop_map(
                    |(call_id, approval_id, turn_id, command, cwd, reason, parsed_cmd)| {
                        EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                            call_id,
                            approval_id,
                            turn_id,
                            command,
                            cwd,
                            reason,
                            network_approval_context: None,
                            proposed_execpolicy_amendment: None,
                            proposed_network_policy_amendments: None,
                            additional_permissions: None,
                            available_decisions: None,
                            parsed_cmd,
                        })
                    }
                ),
            // ApplyPatchApprovalRequest
            (
                arb_safe_string(),
                arb_safe_string(),
                prop::collection::hash_map(arb_pathbuf(), arb_file_change(), 0..2),
                prop::option::of(arb_safe_string()),
                prop::option::of(arb_pathbuf()),
            )
                .prop_map(|(call_id, turn_id, changes, reason, grant_root)| {
                    EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                        call_id,
                        turn_id,
                        changes,
                        reason,
                        grant_root,
                    })
                }),
            // ElicitationRequest
            (
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string(),
                prop::option::of(arb_json_object())
            )
                .prop_map(|(server_name, request_id, message, schema)| {
                    EventMsg::ElicitationRequest(ElicitationRequestEvent {
                        server_name,
                        request_id,
                        message,
                        schema,
                    })
                }),
            // PatchApplyBegin
            (
                arb_safe_string(),
                arb_safe_string(),
                any::<bool>(),
                prop::collection::hash_map(arb_pathbuf(), arb_file_change(), 0..2)
            )
                .prop_map(|(call_id, turn_id, auto_approved, changes)| {
                    EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                        call_id,
                        turn_id,
                        auto_approved,
                        changes,
                    })
                }),
            // PatchApplyEnd
            (
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string(),
                any::<bool>(),
                prop::collection::hash_map(arb_pathbuf(), arb_file_change(), 0..2),
                arb_patch_apply_status(),
            )
                .prop_map(
                    |(call_id, turn_id, stdout, stderr, success, changes, status)| {
                        EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                            call_id,
                            turn_id,
                            stdout,
                            stderr,
                            success,
                            changes,
                            status,
                        })
                    }
                ),
            // DynamicToolCallRequest
            arb_dynamic_tool_call_request().prop_map(EventMsg::DynamicToolCallRequest),
            // DynamicToolCallResponse
            (
                arb_safe_string(),
                arb_safe_string(),
                arb_safe_string(),
                arb_json_object(),
                prop::collection::vec(arb_dynamic_tool_call_output_content_item(), 0..2),
                any::<bool>(),
                prop::option::of(arb_safe_string()),
                prop::option::of(arb_duration()),
            )
                .prop_map(
                    |(
                        call_id,
                        turn_id,
                        tool,
                        arguments,
                        content_items,
                        success,
                        error,
                        duration,
                    )| {
                        EventMsg::DynamicToolCallResponse(DynamicToolCallResponseEvent {
                            call_id,
                            turn_id,
                            tool,
                            arguments,
                            content_items,
                            success,
                            error,
                            duration,
                        })
                    }
                ),
            // UndoStarted
            prop::option::of(arb_safe_string())
                .prop_map(|message| EventMsg::UndoStarted(UndoStartedEvent { message })),
            // UndoCompleted
            (any::<bool>(), prop::option::of(arb_safe_string())).prop_map(|(success, message)| {
                EventMsg::UndoCompleted(UndoCompletedEvent { success, message })
            }),
            // StreamError
            (
                arb_safe_string(),
                prop::option::of(arb_codex_error_info()),
                prop::option::of(arb_safe_string())
            )
                .prop_map(|(message, codex_error_info, additional_details)| {
                    EventMsg::StreamError(StreamErrorEvent {
                        message,
                        codex_error_info,
                        additional_details,
                    })
                }),
            // DeprecationNotice
            (arb_safe_string(), prop::option::of(arb_safe_string())).prop_map(
                |(summary, details)| {
                    EventMsg::DeprecationNotice(DeprecationNoticeEvent { summary, details })
                }
            ),
            // TurnDiff
            arb_safe_string()
                .prop_map(|unified_diff| EventMsg::TurnDiff(TurnDiffEvent { unified_diff })),
        ]
    }

    // ── Roundtrip helper ─────────────────────────────────────────────

    fn assert_roundtrip<T>(value: &T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let decoded: T = serde_json::from_str(&json).unwrap();
        assert_eq!(*value, decoded, "Roundtrip failed.\nJSON: {json}");
    }

    // ── Property tests ───────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn sandbox_policy_roundtrip(v in arb_sandbox_policy()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn ask_for_approval_roundtrip(v in arb_ask_for_approval()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn review_decision_roundtrip(v in arb_review_decision()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn effort_roundtrip(v in arb_effort()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn service_tier_roundtrip(v in arb_service_tier()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn collaboration_mode_roundtrip(v in arb_collaboration_mode()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn personality_roundtrip(v in arb_personality()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn user_input_roundtrip(v in arb_user_input()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn content_item_roundtrip(v in arb_content_item()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn content_or_items_roundtrip(v in arb_content_or_items()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn dynamic_tool_spec_roundtrip(v in arb_dynamic_tool_spec()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn dynamic_tool_call_request_roundtrip(v in arb_dynamic_tool_call_request()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn file_change_roundtrip(v in arb_file_change()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn mcp_invocation_roundtrip(v in arb_mcp_invocation()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn token_usage_roundtrip(v in arb_token_usage()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn turn_context_overrides_roundtrip(v in arb_turn_context_overrides()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn submission_roundtrip(v in arb_submission()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn op_roundtrip(v in arb_op()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn override_turn_context_op_roundtrip(v in arb_override_turn_context_op()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn event_roundtrip(v in arb_event()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn event_msg_roundtrip(v in arb_event_msg()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn event_msg_extended_roundtrip(v in arb_event_msg_extended()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn codex_error_info_roundtrip(v in arb_codex_error_info()) {
            assert_roundtrip(&v);
        }

        #[test]
        fn response_input_item_roundtrip(v in arb_response_input_item()) {
            assert_roundtrip(&v);
        }
    }
}
