/**
 * TypeScript type definitions mirroring the Rust backend protocol types.
 *
 * These types correspond to the serde-serialized JSON emitted by the Tauri
 * backend via IPC events. All tagged enums use `{ type: "variant_name", ... }`.
 */

// ── Primitives & Shared ──────────────────────────────────────────

export interface TokenUsage {
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
}

export interface TokenUsageInfo {
  total_token_usage: TokenUsage;
  last_token_usage: TokenUsage;
  model_context_window: number | null;
}

export interface RateLimitWindow {
  limit: number;
  remaining: number;
  reset: string;
}

export interface CreditsSnapshot {
  remaining: number;
  granted: number;
}

export interface RateLimitSnapshot {
  limit_id?: string;
  limit_name?: string;
  primary?: RateLimitWindow;
  secondary?: RateLimitWindow;
  credits?: CreditsSnapshot;
}

export interface ByteRange {
  start: number;
  end: number;
}

export interface TextElement {
  byte_range: ByteRange;
  placeholder?: string;
}

export type MessagePhase = "Commentary" | "FinalAnswer";

export type ModeKind = "Plan" | "Default";

export type TurnAbortReason = "Interrupted" | "Replaced" | "ReviewEnded";

export type ExecCommandSource =
  | "Agent"
  | "UserShell"
  | "UnifiedExecStartup"
  | "UnifiedExecInteraction";

export type ExecCommandStatus = "completed" | "failed" | "declined";

export type PatchApplyStatus = "completed" | "failed" | "declined";

export type ExecOutputStream = "Stdout" | "Stderr";

export type LocalShellStatus = "Completed" | "InProgress" | "Incomplete";

export type ModelRerouteReason = "high_risk_cyber_activity";

// ── UserInput (tagged: type) ─────────────────────────────────────

export type UserInput =
  | { type: "text"; text: string; text_elements: TextElement[] }
  | { type: "image"; image_url: string }
  | { type: "local_image"; path: string }
  | { type: "skill"; name: string; path: string }
  | { type: "mention"; name: string; path: string };

// ── ContentItem (tagged: type) ───────────────────────────────────

export type ContentItem =
  | { type: "input_text"; text: string }
  | { type: "input_image"; image_url: string }
  | { type: "output_text"; text: string };

// ── WebSearchAction (tagged: type) ───────────────────────────────

export type WebSearchAction =
  | { type: "search"; query?: string; queries?: string[] }
  | { type: "open_page"; url?: string }
  | { type: "find_in_page"; url?: string; pattern?: string }
  | { type: "other" };

// ── LocalShellAction (tagged: type) ──────────────────────────────

export interface LocalShellExecAction {
  command: string[];
  timeout_ms?: number;
  working_directory?: string;
  env?: Record<string, string>;
  user?: string;
}

export type LocalShellAction = { type: "exec" } & LocalShellExecAction;

// ── FunctionCallOutputPayload ────────────────────────────────────

export type FunctionCallOutputContentItem =
  | { type: "input_text"; text: string }
  | { type: "input_image"; image_url: string };

export type FunctionCallOutputBody =
  | string
  | FunctionCallOutputContentItem[];

export interface FunctionCallOutputPayload {
  body: FunctionCallOutputBody;
  success: boolean;
}

// ── ResponseItem (tagged: type) ──────────────────────────────────

export type ResponseItem =
  | {
      type: "message";
      id: string;
      role: string;
      content: ContentItem[];
      end_turn?: boolean;
      phase?: MessagePhase;
    }
  | {
      type: "reasoning";
      id: string;
      summary?: unknown[];
      content?: unknown[];
      encrypted_content?: string;
    }
  | {
      type: "function_call";
      id: string;
      name: string;
      arguments: string;
      call_id: string;
    }
  | {
      type: "function_call_output";
      call_id: string;
      output: FunctionCallOutputPayload;
    }
  | {
      type: "local_shell_call";
      id: string;
      call_id: string;
      status: LocalShellStatus;
      action: LocalShellAction;
    }
  | {
      type: "custom_tool_call";
      id: string;
      status: string;
      call_id: string;
      name: string;
      input: unknown;
    }
  | {
      type: "custom_tool_call_output";
      call_id: string;
      output: FunctionCallOutputPayload;
    }
  | {
      type: "web_search_call";
      id: string;
      status: string;
      action?: WebSearchAction;
    }
  | { type: "ghost_snapshot"; ghost_commit: string }
  | { type: "compaction"; encrypted_content: string }
  | { type: "other" };

// ── TurnItem (tagged: type) ──────────────────────────────────────

export interface AgentMessageContent {
  type: "Text";
  text: string;
}

export type TurnItem =
  | {
      type: "UserMessage";
      id: string;
      content: UserInput[];
    }
  | {
      type: "AgentMessage";
      id: string;
      content: AgentMessageContent[];
      phase?: MessagePhase;
    }
  | { type: "Plan"; id: string; text: string }
  | {
      type: "Reasoning";
      id: string;
      summary_text: string[];
      raw_content: string[];
    }
  | {
      type: "WebSearch";
      id: string;
      query: string;
      action: WebSearchAction;
    }
  | { type: "ContextCompaction"; id: string };

// ── Event payload structs ────────────────────────────────────────

export interface ErrorEvent {
  message: string;
  codex_error_info?: unknown;
}

export interface WarningEvent {
  message: string;
}

export interface SessionConfiguredEvent {
  session_id: string;
  forked_from_id?: string;
  thread_name?: string;
  model: string;
  model_provider_id: string;
  approval_policy?: string;
  sandbox_policy?: unknown;
  cwd: string;
  history_log_id: number;
  history_entry_count: number;
  mode: ModeKind;
  reasoning_effort?: string;
  reasoning_summary?: string;
  can_append: boolean;
}

export interface TurnStartedEvent {
  turn_id: string;
  model_context_window?: number;
  collaboration_mode_kind: ModeKind;
}

export interface TurnCompleteEvent {
  turn_id: string;
  last_agent_message?: string;
}

export interface TurnAbortedEvent {
  turn_id?: string;
  reason: TurnAbortReason;
}

export interface TokenCountEvent {
  info?: TokenUsageInfo;
  rate_limits?: RateLimitSnapshot;
}

export interface AgentMessageEvent {
  message: string;
  phase?: MessagePhase;
}

export interface AgentMessageDeltaEvent {
  delta: string;
}

export interface UserMessageEvent {
  message: string;
  images?: string[];
  local_images: string[];
  text_elements: TextElement[];
}

export interface AgentReasoningEvent {
  text: string;
}

export interface AgentReasoningDeltaEvent {
  delta: string;
}

export interface AgentReasoningRawContentEvent {
  text: string;
}

export interface AgentReasoningRawContentDeltaEvent {
  delta: string;
}

export interface AgentReasoningSectionBreakEvent {
  item_id: string;
  summary_index: number;
}

export interface ItemStartedEvent {
  thread_id: string;
  turn_id: string;
  item: TurnItem;
}

export interface ItemCompletedEvent {
  thread_id: string;
  turn_id: string;
  item: TurnItem;
}

export interface AgentMessageContentDeltaEvent {
  thread_id: string;
  turn_id: string;
  item_id: string;
  delta: string;
}

export interface PlanDeltaEvent {
  thread_id: string;
  turn_id: string;
  item_id: string;
  delta: string;
}

export interface ReasoningContentDeltaEvent {
  thread_id: string;
  turn_id: string;
  item_id: string;
  delta: string;
  summary_index: number;
}

export interface ReasoningRawContentDeltaEvent {
  thread_id: string;
  turn_id: string;
  item_id: string;
  delta: string;
  content_index: number;
}

export interface RawResponseItemEvent {
  item: ResponseItem;
}

export interface ParsedCommand {
  program: string;
  args: string[];
}

export interface ExecCommandBeginEvent {
  call_id: string;
  process_id?: string;
  turn_id: string;
  command: string[];
  cwd: string;
  parsed_cmd: ParsedCommand[];
  source: ExecCommandSource;
  interaction_input?: string;
}

export interface ExecCommandEndEvent {
  call_id: string;
  process_id?: string;
  turn_id: string;
  command: string[];
  cwd: string;
  parsed_cmd: ParsedCommand[];
  source: ExecCommandSource;
  interaction_input?: string;
  stdout: string;
  stderr: string;
  aggregated_output: string;
  exit_code: number;
  duration: { secs: number; nanos: number };
  formatted_output: string;
  status: ExecCommandStatus;
}

export interface ExecCommandOutputDeltaEvent {
  call_id: string;
  delta: string;
  stream?: ExecOutputStream;
}

export interface ExecApprovalRequestEvent {
  call_id: string;
  approval_id?: string;
  turn_id: string;
  command: string[];
  cwd: string;
  reason?: string;
  parsed_cmd: ParsedCommand[];
}

export interface FileChange {
  old_path?: string;
  new_path?: string;
  additions: number;
  deletions: number;
  patch?: string;
}

export interface ApplyPatchApprovalRequestEvent {
  call_id: string;
  turn_id: string;
  changes: Record<string, FileChange>;
  reason?: string;
  grant_root?: string;
}

export interface PatchApplyBeginEvent {
  call_id: string;
  turn_id: string;
  auto_approved: boolean;
  changes: Record<string, FileChange>;
}

export interface PatchApplyEndEvent {
  call_id: string;
  turn_id: string;
  stdout: string;
  stderr: string;
  success: boolean;
  changes: Record<string, FileChange>;
  status: PatchApplyStatus;
}

export interface McpInvocation {
  server_name: string;
  tool_name: string;
  arguments: unknown;
}

export interface McpToolCallBeginEvent {
  call_id: string;
  invocation: McpInvocation;
}

export interface McpToolCallEndEvent {
  call_id: string;
  invocation: McpInvocation;
  duration: { secs: number; nanos: number };
  result: unknown;
}

export interface McpStartupUpdateEvent {
  server: string;
  status: unknown;
}

export interface McpStartupCompleteEvent {
  ready: string[];
  failed: unknown[];
  cancelled: string[];
}

export interface WebSearchBeginEvent {
  call_id: string;
}

export interface WebSearchEndEvent {
  call_id: string;
  query: string;
  action: WebSearchAction;
}

export interface StreamErrorEvent {
  message: string;
  codex_error_info?: unknown;
  additional_details?: string;
}

export interface StreamInfoEvent {
  message: string;
}

export interface ModelRerouteEvent {
  from_model: string;
  to_model: string;
  reason: ModelRerouteReason;
}

export interface CollabAgentSpawnBeginEvent {
  call_id: string;
  sender_thread_id: string;
  prompt: string;
}

export interface CollabAgentSpawnEndEvent {
  call_id: string;
  agents: unknown[];
}

export interface CollabAgentInteractionBeginEvent {
  call_id: string;
  sender_thread_id: string;
  receiver_thread_id: string;
}

export interface CollabAgentInteractionEndEvent {
  call_id: string;
  sender_thread_id: string;
  receiver_thread_id: string;
}

// ── EventMsg (tagged: type, snake_case) ──────────────────────────

export type EventMsg =
  // Errors & warnings
  | { type: "error" } & ErrorEvent
  | { type: "warning" } & WarningEvent
  // Turn lifecycle (note: serde renames to task_started/task_complete)
  | { type: "task_started" } & TurnStartedEvent
  | { type: "task_complete" } & TurnCompleteEvent
  | { type: "turn_aborted" } & TurnAbortedEvent
  // Token usage
  | { type: "token_count" } & TokenCountEvent
  // Agent messages
  | { type: "agent_message" } & AgentMessageEvent
  | { type: "user_message" } & UserMessageEvent
  | { type: "agent_message_delta" } & AgentMessageDeltaEvent
  | { type: "agent_reasoning" } & AgentReasoningEvent
  | { type: "agent_reasoning_delta" } & AgentReasoningDeltaEvent
  | { type: "agent_reasoning_raw_content" } & AgentReasoningRawContentEvent
  | { type: "agent_reasoning_raw_content_delta" } & AgentReasoningRawContentDeltaEvent
  | { type: "agent_reasoning_section_break" } & AgentReasoningSectionBreakEvent
  // Session
  | { type: "session_configured" } & SessionConfiguredEvent
  | { type: "thread_name_updated"; thread_id: string; thread_name?: string }
  // Model reroute
  | { type: "model_reroute" } & ModelRerouteEvent
  // Context
  | { type: "context_compacted" }
  | { type: "thread_rolled_back"; num_turns: number }
  // MCP
  | { type: "mcp_startup_update" } & McpStartupUpdateEvent
  | { type: "mcp_startup_complete" } & McpStartupCompleteEvent
  | { type: "mcp_tool_call_begin" } & McpToolCallBeginEvent
  | { type: "mcp_tool_call_end" } & McpToolCallEndEvent
  | { type: "mcp_list_tools_response"; tools: Record<string, unknown> }
  // Web search
  | { type: "web_search_begin" } & WebSearchBeginEvent
  | { type: "web_search_end" } & WebSearchEndEvent
  // Command execution
  | { type: "exec_command_begin" } & ExecCommandBeginEvent
  | { type: "exec_command_output_delta" } & ExecCommandOutputDeltaEvent
  | { type: "terminal_interaction"; call_id: string; process_id: string; stdin: string }
  | { type: "exec_command_end" } & ExecCommandEndEvent
  // Approval
  | { type: "exec_approval_request" } & ExecApprovalRequestEvent
  | { type: "apply_patch_approval_request" } & ApplyPatchApprovalRequestEvent
  | { type: "request_user_input"; id: string; message: string; schema?: unknown }
  | { type: "elicitation_request"; server_name: string; request_id: string; message: string; schema?: unknown }
  // Patch
  | { type: "patch_apply_begin" } & PatchApplyBeginEvent
  | { type: "patch_apply_end" } & PatchApplyEndEvent
  // Structured items
  | { type: "raw_response_item" } & RawResponseItemEvent
  | { type: "item_started" } & ItemStartedEvent
  | { type: "item_completed" } & ItemCompletedEvent
  | { type: "agent_message_content_delta" } & AgentMessageContentDeltaEvent
  | { type: "plan_delta" } & PlanDeltaEvent
  | { type: "reasoning_content_delta" } & ReasoningContentDeltaEvent
  | { type: "reasoning_raw_content_delta" } & ReasoningRawContentDeltaEvent
  // Stream
  | { type: "stream_error" } & StreamErrorEvent
  | { type: "stream_info" } & StreamInfoEvent
  // Collab
  | { type: "collab_agent_spawn_begin" } & CollabAgentSpawnBeginEvent
  | { type: "collab_agent_spawn_end" } & CollabAgentSpawnEndEvent
  | { type: "collab_agent_interaction_begin" } & CollabAgentInteractionBeginEvent
  | { type: "collab_agent_interaction_end" } & CollabAgentInteractionEndEvent
  | { type: "collab_waiting_begin"; call_id: string; thread_id: string }
  | { type: "collab_waiting_end"; call_id: string; thread_id: string }
  | { type: "collab_close_begin"; call_id: string; thread_id: string }
  | { type: "collab_close_end"; call_id: string; thread_id: string }
  | { type: "collab_resume_begin"; call_id: string; thread_id: string }
  | { type: "collab_resume_end"; call_id: string; thread_id: string }
  // Misc
  | { type: "shutdown_complete" }
  | { type: "skills_update_available" }
  | { type: "background_event"; message: string }
  | { type: "deprecation_notice"; summary: string; details?: string }
  | { type: "dynamic_tool_call_request"; call_id: string; turn_id: string; tool: string; arguments: unknown }
  | { type: "dynamic_tool_call_response"; call_id: string; turn_id: string; tool: string; arguments: unknown; content_items: unknown[]; success: boolean; error?: string }
  | { type: "undo_started"; message?: string }
  | { type: "undo_completed"; success: boolean; message?: string }
  | { type: "turn_diff"; unified_diff: string }
  | { type: "plan_update"; plan: unknown }
  | { type: "view_image_tool_call"; call_id: string; path: string }
  | { type: "conversation_path_response"; conversation_id: string; path: string }
  | { type: "realtime_conversation_started"; session_id?: string }
  | { type: "realtime_conversation_realtime"; event: unknown }
  | { type: "realtime_conversation_closed"; reason?: string };

// ── Event wrapper ────────────────────────────────────────────────

export interface Event {
  id: string;
  msg: EventMsg;
}

// ── Op (submission operations) ───────────────────────────────────

export type Op =
  | {
      type: 'user_turn';
      items: UserInput[];
      cwd: string;
      model: string;
      approval_policy: ApprovalPolicy;
      sandbox_policy: SandboxPolicy;
      effort?: string;
      summary?: string;
      service_tier?: string;
      collaboration_mode?: unknown;
      personality?: string;
    }
  | { type: 'user_input'; items: UserInput[]; final_output_json_schema?: unknown }
  | { type: 'interrupt' }
  | { type: 'shutdown' }
  | { type: 'exec_approval'; id: string; turn_id?: string; decision: string; custom_instructions?: string }
  | { type: 'patch_approval'; id: string; decision: string; custom_instructions?: string }
  | { type: 'compact' }
  | { type: 'undo' }
  | { type: 'thread_rollback'; num_turns: number }
  | { type: 'set_thread_name'; name: string }
  | { type: 'list_mcp_tools' }
  | { type: 'list_skills'; cwds?: string[]; force_reload?: boolean }
  | { type: 'list_models' }
  | { type: 'reload_user_config' };

// ── Approval & Sandbox policies (kebab-case to match Rust serde) ─

export type ApprovalPolicy = 'untrusted' | 'on-failure' | 'on-request' | 'never';

export type SandboxPolicy =
  | { type: 'danger-full-access' }
  | { type: 'read-only'; access: unknown }
  | { type: 'external-sandbox'; network_access: unknown }
  | { type: 'workspace-write'; writable_roots: string[]; read_only_access: unknown; network_access: boolean };
