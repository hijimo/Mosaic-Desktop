# Streaming Message Rendering Design

## Overview

This spec redesigns the streaming assistant message rendering path for the desktop chat UI.

The target is stability first. During AI streaming, the UI must avoid visible jumping even if that means delaying visible updates by one or two animation frames. The current implementation updates React state on nearly every delta event, re-renders the entire streaming area too often, and couples scroll correction with layout changes. That combination produces small but noticeable viewport jumps.

The new design introduces a frame-based streaming pipeline built around `requestAnimationFrame`, buffered event ingestion, segmented subscriptions, and a single bottom-lock scroll coordinator.

## Goals

- Eliminate visible jumpiness during assistant streaming.
- Guarantee that streaming UI updates are frame-bounded instead of delta-bounded.
- Decouple high-frequency event ingestion from lower-frequency React rendering.
- Keep the viewport visually pinned to the bottom when the user is reading the live tail.
- Never steal scroll position when the user has scrolled away from the bottom.
- Preserve final message fidelity after streaming completes.

## Non-Goals

- No attempt to preserve character-by-character immediacy at any cost.
- No DOM-direct dual rendering path outside React for v1 of this redesign.
- No rewrite of completed historical message rendering.
- No visual animation for streaming text if that animation risks layout instability.

## Existing Root Causes

The current jumpiness is not caused by a single bug. It is the result of several mechanisms interacting:

1. **Delta-driven React commits**
   - Streaming deltas are written into Zustand immediately.
   - React consumers re-render at token frequency instead of frame frequency.

2. **Over-broad store subscriptions**
   - `MessageList` subscribes to the entire `streamingTurn`.
   - Agent text, reasoning, plan items, tool calls, approvals, and clarification cards all participate in the same render wave.

3. **Whole-string Markdown recomputation**
   - `Streamdown` receives the entire streaming text repeatedly.
   - As incomplete Markdown becomes syntactically valid, previously rendered regions can reflow.
   - This is especially visible for code fences, lists, and tables.

4. **Scroll correction coupled to content churn**
   - Bottom-follow logic reacts too directly to content changes.
   - Layout changes and scroll writes happen too often and too close together.

5. **Above-the-tail insertions**
   - Reasoning, tool cards, approvals, and clarifications can grow above the streaming answer.
   - Any height change above the active reading point naturally shifts the viewport unless explicitly compensated.

## Design Principles

- **Render at most once per frame per streaming region.**
- **One owner for scroll correction.**
- **Separate input buffering from visible state.**
- **Prefer stable intermediate rendering over aggressive live formatting.**
- **Split streaming UI into independently subscribed islands.**

## Recommended Architecture

The redesign introduces four layers:

1. **Streaming Event Buffer**
   - Collects raw deltas from event handlers.
   - Does not directly trigger visible React updates.

2. **Frame Scheduler**
   - Uses `requestAnimationFrame` to flush pending buffered changes at most once per frame.
   - Produces a consolidated view model for React.

3. **Streaming View State**
   - Holds only the last frame-committed visible state.
   - This is the only state React streaming components subscribe to.

4. **Bottom-Lock Scroll Coordinator**
   - Owns all bottom-follow logic.
   - Decides after frame commits whether the viewport should remain pinned.

This preserves React as the rendering owner while removing token-frequency commits.

## State Model

### 1. Ingestion State

This layer is mutable, internal, and not directly rendered.

Suggested shape:

```ts
interface StreamBufferItem {
  itemId: string;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  pendingAgentText: string;
  pendingReasoningSummary: string[];
  pendingReasoningRaw: string[];
  pendingPlanText: string;
  dirty: boolean;
}

interface StreamBufferTurn {
  turnId: string;
  items: Map<string, StreamBufferItem>;
  dirtyAgentText: boolean;
  dirtyReasoning: boolean;
  dirtyPlan: boolean;
  dirtyStructure: boolean;
}
```

Properties:

- receives all raw deltas immediately,
- merges multiple deltas before render,
- can be mutated many times within a frame,
- does not notify React consumers directly.

### 2. Visible Streaming State

This layer is immutable and React-visible.

Suggested shape:

```ts
interface StreamingViewItem {
  itemId: string;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  agentText: string;
  reasoningSummary: string[];
  planText: string;
}

interface StreamingViewTurn {
  turnId: string;
  isStreaming: boolean;
  itemOrder: string[];
  items: Map<string, StreamingViewItem>;
  revision: number;
}
```

Properties:

- updated only by the frame scheduler,
- safe for narrow React subscriptions,
- monotonic revision used for layout/scroll coordination.

## Frame Scheduler

The scheduler is the heart of the redesign.

### Responsibilities

- collect buffered mutations for the current frame,
- schedule exactly one `requestAnimationFrame` flush,
- commit a stable visible snapshot,
- trigger post-commit scroll reconciliation,
- flush final trailing content when streaming completes.

### Scheduling Rules

1. Event delta arrives.
2. Delta is merged into `StreamBufferTurn`.
3. If no frame is pending, schedule one `requestAnimationFrame`.
4. On frame:
   - gather all dirty buffered changes,
   - apply them to `StreamingViewTurn` in one commit,
   - increment `revision`,
   - request post-commit layout reconciliation.

### Completion Rules

When `task_complete`, `turn_aborted`, or `item_completed` arrives:

- flush any pending buffered content immediately,
- mark the corresponding visible region complete,
- switch rendering from streaming mode to final mode only after the last pending frame data is committed.

### Flush Priority

Not all regions need identical frequency:

- assistant body: flush every animation frame when dirty,
- reasoning summaries: flush no more than every 2 frames or ~50ms,
- plan text: flush every frame when visible,
- tool calls / approvals / clarifications: structure-level updates only, still through the same frame scheduler.

This reduces above-the-tail layout churn without freezing the UI.

## React Component Boundaries

The current `MessageList` component carries too many responsibilities. The redesign splits streaming rendering into isolated islands:

- `StreamingTurnRoot`
- `StreamingReasoningList`
- `StreamingPlanList`
- `StreamingToolRegion`
- `StreamingApprovalRegion`
- `StreamingClarificationRegion`
- `StreamingAgentBody`

### Subscription Rules

Each component subscribes only to the exact data it needs:

- `StreamingAgentBody` subscribes to assistant body text and completion mode.
- `StreamingReasoningList` subscribes only to reasoning summaries.
- `StreamingToolRegion` subscribes only to tool-call view state.
- scroll coordinator subscribes to visible revision plus bottom-lock state.

This prevents body text updates from forcing unrelated UI blocks to re-render.

## Streaming Markdown Strategy

The assistant body is the main source of reflow.

### Streaming Mode

During streaming:

- disable text animation,
- avoid token-level commits,
- prefer conservative Markdown rendering,
- treat incomplete structures as unstable and avoid formatting transitions that re-layout historical content too aggressively.

Practical rules:

- unfinished code fences should remain visually stable,
- partially formed tables should not thrash the earlier layout,
- list normalization should be conservative during streaming,
- no extra streaming animation layered on top of Markdown reparse.

### Final Mode

After the assistant message completes:

- render the final full Markdown once,
- preserve the completed answer exactly,
- allow the final structure to replace the conservative streaming presentation.

This creates a two-stage rendering contract:

- streaming stage favors stability,
- completion stage favors fidelity.

## Scroll Model

Scrolling must become frame-based and centralized.

### Bottom Lock

Track whether the user is actively pinned to the live tail.

Suggested rule:

- `bottomLock = true` when `scrollHeight - scrollTop - clientHeight < threshold`
- threshold remains small and stable, for example 50px

If the user scrolls upward:

- `bottomLock` becomes `false`
- the system must stop all automatic scroll corrections

If the user returns near the bottom:

- `bottomLock` becomes `true`
- future frame commits may again keep the tail pinned

### Scroll Ownership

Only one mechanism may write `scrollTop`.

That mechanism:

- runs after visible frame commit,
- reads layout once,
- writes scroll position once if `bottomLock` is true,
- never writes in multiple independent effects.

### Post-Commit Reconciliation

Recommended sequence:

1. frame flush updates visible state,
2. React commits DOM,
3. scroll coordinator runs in the next frame or post-layout step,
4. if `bottomLock` is true, set `scrollTop = scrollHeight`,
5. clear pending reconciliation.

### Scroll Anchoring

The streaming container must keep:

```css
overflow-anchor: none;
```

This prevents the browser from fighting the custom bottom-follow behavior.

## Above-the-Tail Dynamic Regions

Reasoning panels and tool/approval cards are above the assistant answer and can change height. To keep the viewport stable:

- they must be updated through the same scheduler as the body,
- they should not perform autonomous scroll adjustments,
- their refresh cadence should be lower than token frequency,
- structure changes should be batched with body changes where possible.

If these regions update independently at arbitrary times, even perfect body buffering will still feel jumpy.

## Store and Hook Responsibilities

### `useStreamingScheduler`

New hook or store-adjacent module responsible for:

- buffering incoming deltas,
- coalescing writes,
- scheduling frame flushes,
- publishing visible view state,
- forcing final flush on completion.

### `useBottomLockScroll`

New hook responsible for:

- tracking whether the user is near the bottom,
- scheduling a post-commit scroll reconciliation,
- writing `scrollTop` in one place only,
- cancelling stale frame callbacks.

### `useCodexEvent`

Event handlers should become ingestion-only:

- parse event,
- push to scheduler buffer,
- do not directly create visible token-frequency React state transitions.

## Failure Handling

- If `requestAnimationFrame` is unavailable, fall back to a minimal timer-based frame scheduler with the same batching semantics.
- If streaming ends while a frame is pending, force one last synchronous flush before completion mode.
- If the stream aborts mid-Markdown structure, preserve the last stable visible content and then finalize once abort handling completes.
- If a buffered item is missing from visible order, append it deterministically during the next flush instead of mutating the DOM directly.

## Testing Strategy

### Unit Tests

- scheduler coalesces many deltas into one visible commit,
- scheduler flushes pending data on completion,
- bottom-lock scroll does not fire when the user is away from the bottom,
- bottom-lock scroll fires once after a frame commit when pinned,
- reasoning updates do not force assistant body commits when the body is unchanged,
- body text updates do not force unrelated tool/approval region renders.

### Component Tests

- streaming body updates at frame boundaries, not delta boundaries,
- `StreamingAgentBody` re-renders only when body view state changes,
- reasoning and tool sections stay isolated under body churn,
- final Markdown render replaces conservative streaming mode at completion.

### Behavioral Tests

- while pinned to bottom, long streaming output remains visually stable,
- when reasoning grows above the tail, the viewport does not visibly jump,
- when the user scrolls upward, auto-follow stops immediately,
- when the user returns to the bottom, auto-follow resumes cleanly.

### Manual Verification

- plain text answer,
- long Markdown answer,
- incomplete then completed code fence,
- reasoning-heavy response,
- tool-call-heavy response,
- mixed reasoning plus plan plus answer,
- user scrolls upward during streaming,
- stream aborts mid-answer.

## Success Criteria

The redesign is successful when all of the following are true:

- the streaming body performs at most one visible React commit per animation frame,
- there is no perceptible jumpiness during ordinary assistant streaming while bottom-locked,
- dynamic blocks above the answer no longer cause visible tail instability,
- user-controlled scroll position is respected,
- final completed output matches the full assistant response.

## Implementation Boundaries

The first implementation should stay focused:

- redesign only the live streaming path,
- preserve existing completed message rendering where possible,
- do not introduce a DOM-direct rendering fork,
- do not add speculative animation or cosmetic motion until stability is proven by tests.
