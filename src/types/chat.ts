/**
 * Chat component types for Mosaic Desktop.
 */

/** Tool call tracking state */
export interface ToolCallState {
  callId: string;
  type: 'exec' | 'mcp' | 'web_search' | 'patch';
  status: 'pending' | 'running' | 'completed' | 'failed';
  name: string;
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
  command?: string[];
  cwd?: string;
  reason?: string;
  changes?: Record<string, unknown>;
}

/** Message role for rendering */
export type MessageRole = 'user' | 'agent';
