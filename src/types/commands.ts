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
  thread_id: string;
  cwd?: string;
}

export interface ThreadForkParams {
  source_thread_id: string;
  new_thread_id: string;
  cwd?: string;
}

// ── Submit op ────────────────────────────────────────────────────

export interface SubmitOpParams {
  thread_id: string;
  id: string;
  op: Record<string, unknown>;
}

// ── Fuzzy file search ────────────────────────────────────────────

export interface FuzzyFileSearchParams {
  query: string;
  roots: string[];
}

export type FuzzyFileSearchResponse = FileMatch[];
