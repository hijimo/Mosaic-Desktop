/**
 * Tauri IPC command types for the Mosaic Desktop frontend.
 */
import type { Event } from "./events";
import type { FileMatch } from "./file-search";

// ── Event bridge payload (from app_handle.emit) ──────────────────

/** Payload received via `listen("codex-event", ...)` */
export interface CodexEventPayload {
  thread_id: string;
  event: Event;
}

// ── Thread management ────────────────────────────────────────────

export interface ThreadStartParams {
  cwd?: string;
}

export interface ThreadForkParams {
  sourceThreadId: string;
  cwd?: string;
}

/** Metadata for a thread, returned by thread_get_info. */
export interface ThreadMeta {
  thread_id: string;
  cwd: string;
  /** Populated after session_configured event is received. */
  model: string | null;
  model_provider_id: string | null;
  name: string | null;
  created_at: string; // ISO 8601
  forked_from: string | null;
}

// ── Submit op ────────────────────────────────────────────────────

export interface SubmitOpParams {
  threadId: string;
  id: string;
  op: Record<string, unknown>;
}

// ── Fuzzy file search ────────────────────────────────────────────

export interface FuzzyFileSearchParams {
  query: string;
  roots: string[];
}

export type FuzzyFileSearchResponse = FileMatch[];
