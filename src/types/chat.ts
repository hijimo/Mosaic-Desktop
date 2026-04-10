/**
 * Chat component types for Mosaic Desktop.
 */

/** Tool call tracking state */
export interface ToolCallState {
  callId: string;
  type: 'exec' | 'mcp' | 'web_search' | 'patch';
  status: 'pending' | 'running' | 'completed' | 'failed';
  name: string;
  order?: number;
  command?: string[];
  cwd?: string;
  output?: string;
  exitCode?: number;
  serverName?: string;
  toolName?: string;
  arguments?: unknown;
  result?: unknown;
}

/** Approval request state */
export interface ApprovalRequestState {
  callId: string;
  turnId: string;
  type: 'exec' | 'patch';
  order?: number;
  command?: string[];
  cwd?: string;
  reason?: string;
  changes?: Record<string, unknown>;
  availableDecisions?: ReviewDecision[];
}

/** Review decision types matching Rust ReviewDecision serde format */
export type ReviewDecision =
  | 'approved'
  | 'approved_for_session'
  | { approved_execpolicy_amendment: { proposed_execpolicy_amendment: string[] } }
  | { network_policy_amendment: { network_policy_amendment: NetworkPolicyAmendment } }
  | 'denied'
  | 'abort';

export interface NetworkPolicyAmendment {
  host: string;
  action: 'allow' | 'deny';
}

/** MCP elicitation request state */
export interface ElicitationRequestState {
  serverName: string;
  requestId: string;
  message: string;
  /** "form" or "url". Absent means "form". */
  mode?: 'form' | 'url';
  /** JSON Schema for form mode. */
  schema?: Record<string, unknown>;
  /** Target URL for url mode. */
  url?: string;
  order?: number;
}

/** Message role for rendering */
export type MessageRole = 'user' | 'agent';

/** Clarification request state */
export interface ClarificationState {
  id: string;
  order?: number;
  message: string;
  schema?: unknown;
}
