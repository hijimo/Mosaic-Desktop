# Streaming Message Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the live assistant streaming path so visible updates are frame-bounded, bottom-follow is stable, and the chat viewport no longer jumps during streaming.

**Architecture:** Introduce a buffered streaming scheduler between event ingestion and React rendering, split the streaming UI into narrowly subscribed regions, and centralize all scroll correction behind a single bottom-lock coordinator. Streaming Markdown uses a stability-first mode during live updates and finalizes once the assistant message completes.

**Tech Stack:** React 19, Zustand, TypeScript, Vitest, MUI, Streamdown

---

## File Structure

### New files

- `src/hooks/useBottomLockScroll.ts`
  - Owns bottom-lock state, post-commit scroll reconciliation, and `requestAnimationFrame` cleanup.
- `src/components/chat/streaming/StreamingTurnRoot.tsx`
  - Layout shell for the live streaming area.
- `src/components/chat/streaming/StreamingReasoningList.tsx`
  - Renders reasoning summaries only.
- `src/components/chat/streaming/StreamingPlanList.tsx`
  - Renders streaming plan items only.
- `src/components/chat/streaming/StreamingToolRegion.tsx`
  - Renders active tool calls only.
- `src/components/chat/streaming/StreamingApprovalRegion.tsx`
  - Renders approvals only.
- `src/components/chat/streaming/StreamingClarificationRegion.tsx`
  - Renders clarifications only.
- `src/components/chat/streaming/StreamingAgentBody.tsx`
  - Renders the live assistant answer only.
- `src/__tests__/unit/hooks/useBottomLockScroll.test.ts`
  - Verifies bottom-lock and frame-based scroll behavior.
- `src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx`
  - Verifies frame-bounded body updates and streaming/final mode switching.

### Modified files

- `src/stores/messageStore.ts`
  - Add buffer/view separation and scheduler-driven flush API.
- `src/hooks/useCodexEvent.ts`
  - Stop applying visible token-frequency updates directly; route events into the scheduler-facing store API.
- `src/components/chat/MessageList.tsx`
  - Remove ad-hoc streaming rendering and delegate to streaming subcomponents and bottom-lock hook.
- `src/components/chat/shared/StreamdownRenderer.tsx`
  - Add stability-first streaming mode and disable risky live animation.
- `src/__tests__/unit/stores/messageStore.test.ts`
  - Cover buffer coalescing, visible flushes, and completion flushes.
- `src/__tests__/unit/hooks/useCodexEvent.test.ts`
  - Cover event ingestion into the buffered scheduler path.
- `src/__tests__/unit/components/MessageList.test.tsx`
  - Replace direct scroll-on-delta assertions with frame-based streaming assertions.

## Task 1: Rebuild `messageStore` around buffer/view separation

**Files:**
- Create: none
- Modify: `src/stores/messageStore.ts`
- Test: `src/__tests__/unit/stores/messageStore.test.ts`

- [ ] **Step 1: Write the failing scheduler store tests**

Add tests that pin down the new contract:

```ts
it('buffers multiple agent deltas until flushVisibleStreaming runs', () => {
  const store = useMessageStore.getState();
  store.startStreaming('turn-1');
  store.startStreamingItem('thread-1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });

  store.bufferAgentContentDelta('a1', 'Hel');
  store.bufferAgentContentDelta('a1', 'lo');

  expect(store.streamingView?.items.get('a1')?.agentText ?? '').toBe('');

  store.flushVisibleStreaming();

  expect(useMessageStore.getState().streamingView?.items.get('a1')?.agentText).toBe('Hello');
});

it('flushes buffered content before stopStreaming finalizes the visible turn', () => {
  const store = useMessageStore.getState();
  store.startStreaming('turn-1');
  store.startStreamingItem('thread-1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });

  store.bufferAgentContentDelta('a1', 'done');
  store.stopStreaming();

  expect(useMessageStore.getState().streamingView?.items.get('a1')?.agentText).toBe('done');
  expect(useMessageStore.getState().streamingView?.isStreaming).toBe(false);
});
```

- [ ] **Step 2: Run the store tests to verify they fail**

Run:

```bash
npm test -- src/__tests__/unit/stores/messageStore.test.ts
```

Expected:

- FAIL because `bufferAgentContentDelta`, `flushVisibleStreaming`, and `streamingView` do not exist yet.

- [ ] **Step 3: Add buffer/view state and flush APIs to `messageStore`**

Refactor `src/stores/messageStore.ts` so the store exposes a mutable buffer layer and an immutable visible layer. The implementation should look like this shape:

```ts
interface StreamingBufferItem {
  threadId: string;
  turnId: string;
  itemId: string;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  pendingAgentText: string;
  pendingReasoningSummary: string[];
  pendingReasoningRaw: string[];
  pendingPlanText: string;
  dirty: boolean;
}

interface StreamingViewItem {
  itemId: string;
  itemType: 'AgentMessage' | 'Reasoning' | 'Plan';
  agentText: string;
  reasoningSummary: string[];
  reasoningRaw: string[];
  planText: string;
}

bufferAgentContentDelta: (itemId, delta) =>
  set((state) => {
    if (!state.streamingBuffer) return state;
    const items = new Map(state.streamingBuffer.items);
    const item = items.get(itemId);
    if (!item) return state;

    items.set(itemId, {
      ...item,
      pendingAgentText: item.pendingAgentText + delta,
      dirty: true,
    });

    return {
      streamingBuffer: {
        ...state.streamingBuffer,
        items,
        dirtyBody: true,
      },
    };
  }),

flushVisibleStreaming: () =>
  set((state) => {
    if (!state.streamingBuffer || !state.streamingView) return state;

    const nextItems = new Map(state.streamingView.items);
    for (const [itemId, buffered] of state.streamingBuffer.items) {
      if (!buffered.dirty) continue;

      const prev = nextItems.get(itemId);
      nextItems.set(itemId, {
        itemId,
        itemType: buffered.itemType,
        agentText: (prev?.agentText ?? '') + buffered.pendingAgentText,
        reasoningSummary: mergeTextArrays(prev?.reasoningSummary ?? [], buffered.pendingReasoningSummary),
        reasoningRaw: mergeTextArrays(prev?.reasoningRaw ?? [], buffered.pendingReasoningRaw),
        planText: (prev?.planText ?? '') + buffered.pendingPlanText,
      });
    }

    return {
      streamingBuffer: resetStreamingBuffer(state.streamingBuffer),
      streamingView: {
        ...state.streamingView,
        items: nextItems,
        revision: state.streamingView.revision + 1,
      },
    };
  }),
```

Also update `startStreaming`, `startStreamingItem`, `stopStreaming`, and `completeStreamingItem` so they maintain both buffer and view state. `stopStreaming` must flush pending buffered content before marking the turn complete.

- [ ] **Step 4: Run the store tests to verify they pass**

Run:

```bash
npm test -- src/__tests__/unit/stores/messageStore.test.ts
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add src/stores/messageStore.ts src/__tests__/unit/stores/messageStore.test.ts
git commit -m "refactor: split streaming store into buffer and view"
```

## Task 2: Move event ingestion onto the buffered scheduler path

**Files:**
- Modify: `src/hooks/useCodexEvent.ts`
- Test: `src/__tests__/unit/hooks/useCodexEvent.test.ts`

- [ ] **Step 1: Write the failing event ingestion tests**

Update the hook tests to assert that deltas hit the buffer API first and only become visible after an explicit flush:

```ts
it('buffers agent deltas instead of writing visible text immediately', async () => {
  renderHook(() => useCodexEvent());

  emitCodexEvent({
    thread_id: 'thread-1',
    event: { msg: { type: 'task_started', turn_id: 'turn-1' } },
  });

  emitCodexEvent({
    thread_id: 'thread-1',
    event: { msg: { type: 'item_started', thread_id: 'thread-1', turn_id: 'turn-1', item: { type: 'AgentMessage', id: 'a1', content: [] } } },
  });

  emitCodexEvent({
    thread_id: 'thread-1',
    event: { msg: { type: 'agent_message_content_delta', item_id: 'a1', delta: 'Hi' } },
  });

  expect(useMessageStore.getState().streamingView?.items.get('a1')?.agentText ?? '').toBe('');

  act(() => {
    useMessageStore.getState().flushVisibleStreaming();
  });

  expect(useMessageStore.getState().streamingView?.items.get('a1')?.agentText).toBe('Hi');
});
```

- [ ] **Step 2: Run the event hook tests to verify they fail**

Run:

```bash
npm test -- src/__tests__/unit/hooks/useCodexEvent.test.ts
```

Expected:

- FAIL because `useCodexEvent` still calls the old immediate visible update methods.

- [ ] **Step 3: Route deltas into buffer APIs and flush on completion**

Update `src/hooks/useCodexEvent.ts` so the relevant handlers use the new methods:

```ts
const {
  startStreaming,
  stopStreaming,
  startStreamingItem,
  bufferAgentContentDelta,
  bufferReasoningContentDelta,
  bufferReasoningRawContentDelta,
  bufferPlanDelta,
  flushVisibleStreaming,
  completeStreamingItem,
} = useMessageStore();

case 'agent_message_content_delta':
  bufferAgentContentDelta(msg.item_id, msg.delta);
  break;

case 'reasoning_content_delta':
  bufferReasoningContentDelta(msg.item_id, msg.delta, msg.summary_index);
  break;

case 'plan_delta':
  bufferPlanDelta(msg.item_id, msg.delta);
  break;

case 'task_complete':
case 'turn_aborted':
  flushVisibleStreaming();
  stopStreaming();
  break;

case 'item_completed':
  flushVisibleStreaming();
  completeStreamingItem(thread_id, msg.turn_id, msg.item);
  break;
```

Do not introduce any scheduling logic here. This hook must stay ingestion-only.

- [ ] **Step 4: Run the event hook tests to verify they pass**

Run:

```bash
npm test -- src/__tests__/unit/hooks/useCodexEvent.test.ts
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useCodexEvent.ts src/__tests__/unit/hooks/useCodexEvent.test.ts
git commit -m "refactor: buffer codex streaming deltas before render"
```

## Task 3: Add a frame scheduler and bottom-lock scroll coordinator

**Files:**
- Create: `src/hooks/useBottomLockScroll.ts`
- Modify: `src/components/chat/MessageList.tsx`
- Test: `src/__tests__/unit/hooks/useBottomLockScroll.test.ts`
- Test: `src/__tests__/unit/components/MessageList.test.tsx`

- [ ] **Step 1: Write the failing scroll coordinator tests**

Create `src/__tests__/unit/hooks/useBottomLockScroll.test.ts` with:

```ts
it('writes scrollTop once after scheduleReconcile when bottomLock is true', () => {
  const scrollTopSet = vi.fn();
  const container = makeScrollableElement({ scrollHeight: 1200, clientHeight: 400, onSetScrollTop: scrollTopSet });

  const { result } = renderHook(() => useBottomLockScroll());

  act(() => {
    result.current.attachContainer(container);
    result.current.setBottomLock(true);
    result.current.scheduleReconcile();
  });

  expect(scrollTopSet).toHaveBeenCalledTimes(1);
  expect(scrollTopSet).toHaveBeenCalledWith(1200);
});

it('does not write scrollTop when bottomLock is false', () => {
  const scrollTopSet = vi.fn();
  const container = makeScrollableElement({ scrollHeight: 1200, clientHeight: 400, onSetScrollTop: scrollTopSet });

  const { result } = renderHook(() => useBottomLockScroll());

  act(() => {
    result.current.attachContainer(container);
    result.current.setBottomLock(false);
    result.current.scheduleReconcile();
  });

  expect(scrollTopSet).not.toHaveBeenCalled();
});
```

Update `MessageList.test.tsx` to assert that body text changes alone do not call scroll directly, but a scheduled reconcile after a view `revision` does.

- [ ] **Step 2: Run the scroll tests to verify they fail**

Run:

```bash
npm test -- src/__tests__/unit/hooks/useBottomLockScroll.test.ts src/__tests__/unit/components/MessageList.test.tsx
```

Expected:

- FAIL because the new hook does not exist and `MessageList` still owns scrolling inline.

- [ ] **Step 3: Implement `useBottomLockScroll`**

Create `src/hooks/useBottomLockScroll.ts`:

```ts
import { useCallback, useEffect, useRef, useState } from 'react';

const BOTTOM_THRESHOLD = 50;

export function useBottomLockScroll() {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameRef = useRef<number | null>(null);
  const [bottomLock, setBottomLock] = useState(true);

  const attachContainer = useCallback((node: HTMLDivElement | null) => {
    containerRef.current = node;
  }, []);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    setBottomLock(el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_THRESHOLD);
  }, []);

  const scheduleReconcile = useCallback(() => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
    }

    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      const el = containerRef.current;
      if (!el || !bottomLock) return;
      el.scrollTop = el.scrollHeight;
    });
  }, [bottomLock]);

  useEffect(() => () => {
    if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
  }, []);

  return { attachContainer, bottomLock, setBottomLock, handleScroll, scheduleReconcile };
}
```

- [ ] **Step 4: Refactor `MessageList` to use the hook and visible `revision`**

Update `src/components/chat/MessageList.tsx` so it stops reading live text for scroll behavior. The key shape should become:

```tsx
const streamingView = useMessageStore((s) => s.streamingView);
const revision = streamingView?.revision ?? 0;
const { attachContainer, handleScroll, scheduleReconcile } = useBottomLockScroll();

useLayoutEffect(() => {
  scheduleReconcile();
}, [revision, scheduleReconcile]);

return (
  <Box
    ref={attachContainer}
    onScroll={handleScroll}
    sx={{ flex: 1, overflow: 'auto', overflowAnchor: 'none', px: 8, pt: 3, pb: 24 }}
  >
    <StreamingTurnRoot threadId={threadId} onApprovalDecision={onApprovalDecision} />
  </Box>
);
```

Do not let any child component write `scrollTop`.

- [ ] **Step 5: Run the scroll tests to verify they pass**

Run:

```bash
npm test -- src/__tests__/unit/hooks/useBottomLockScroll.test.ts src/__tests__/unit/components/MessageList.test.tsx
```

Expected:

- PASS

- [ ] **Step 6: Commit**

```bash
git add src/hooks/useBottomLockScroll.ts src/components/chat/MessageList.tsx src/__tests__/unit/hooks/useBottomLockScroll.test.ts src/__tests__/unit/components/MessageList.test.tsx
git commit -m "refactor: centralize bottom-lock scroll reconciliation"
```

## Task 4: Split the streaming UI into isolated rendering islands

**Files:**
- Create: `src/components/chat/streaming/StreamingTurnRoot.tsx`
- Create: `src/components/chat/streaming/StreamingReasoningList.tsx`
- Create: `src/components/chat/streaming/StreamingPlanList.tsx`
- Create: `src/components/chat/streaming/StreamingToolRegion.tsx`
- Create: `src/components/chat/streaming/StreamingApprovalRegion.tsx`
- Create: `src/components/chat/streaming/StreamingClarificationRegion.tsx`
- Create: `src/components/chat/streaming/StreamingAgentBody.tsx`
- Modify: `src/components/chat/MessageList.tsx`
- Test: `src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx`
- Test: `src/__tests__/unit/components/MessageList.test.tsx`

- [ ] **Step 1: Write the failing isolated rendering tests**

Create `StreamingAgentBody.test.tsx`:

```ts
it('renders buffered body text only after visible flushes', () => {
  seedStreamingView({
    turnId: 'turn-1',
    isStreaming: true,
    items: new Map([
      ['a1', { itemId: 'a1', itemType: 'AgentMessage', agentText: 'Hello', reasoningSummary: [], reasoningRaw: [], planText: '' }],
    ]),
  });

  render(<StreamingAgentBody />);

  expect(screen.getByText('Hello')).toBeInTheDocument();
});

it('switches from streaming mode to final mode when isStreaming becomes false', () => {
  const { rerender } = render(<StreamingAgentBody />);
  seedStreamingView({ ...buildStreamingView(), isStreaming: true });
  rerender(<StreamingAgentBody />);
  seedStreamingView({ ...buildStreamingView(), isStreaming: false });
  rerender(<StreamingAgentBody />);

  expect(mockedStreamdown).toHaveBeenLastCalledWith(expect.objectContaining({ isStreaming: false }), undefined);
});
```

Update `MessageList.test.tsx` so a body revision rerender does not require tool cards to remount.

- [ ] **Step 2: Run the component tests to verify they fail**

Run:

```bash
npm test -- src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx src/__tests__/unit/components/MessageList.test.tsx
```

Expected:

- FAIL because the new components do not exist yet.

- [ ] **Step 3: Implement the streaming region components**

Create `StreamingTurnRoot.tsx` as the composition layer:

```tsx
export function StreamingTurnRoot({ threadId, onApprovalDecision }: StreamingTurnRootProps) {
  const streamingView = useMessageStore((s) => s.streamingView);
  const isStreaming = streamingView?.isStreaming ?? false;

  return (
    <>
      {(isStreaming || hasCompletedTurns(threadId)) && <TaskStartedIndicator />}
      {renderCompletedTurns(threadId, onApprovalDecision)}
      {isStreaming ? (
        <>
          <StreamingReasoningList />
          <StreamingPlanList />
          <StreamingToolRegion />
          <StreamingApprovalRegion onApprovalDecision={onApprovalDecision} />
          <StreamingClarificationRegion />
          <StreamingAgentBody />
        </>
      ) : null}
      {!isStreaming && hasCompletedTurns(threadId) ? <TaskCompletedIndicator /> : null}
    </>
  );
}
```

Each region must subscribe only to the state slice it needs. `StreamingAgentBody` must read only the agent body item text, not reasoning or tool state.

- [ ] **Step 4: Run the component tests to verify they pass**

Run:

```bash
npm test -- src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx src/__tests__/unit/components/MessageList.test.tsx
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/chat/MessageList.tsx src/components/chat/streaming src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx src/__tests__/unit/components/MessageList.test.tsx
git commit -m "refactor: split live streaming UI into isolated regions"
```

## Task 5: Make `StreamdownRenderer` stability-first during live streaming

**Files:**
- Modify: `src/components/chat/shared/StreamdownRenderer.tsx`
- Modify: `src/components/chat/streaming/StreamingAgentBody.tsx`
- Test: `src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx`

- [ ] **Step 1: Write the failing streaming Markdown stability tests**

Add tests asserting that streaming mode disables risky animation and final mode restores normal rendering intent:

```ts
it('passes non-animated stability-first props to Streamdown while streaming', () => {
  render(<StreamingAgentBody />);

  expect(mockedStreamdown).toHaveBeenCalledWith(
    expect.objectContaining({
      isAnimating: false,
      children: 'Hello',
    }),
    undefined,
  );
});
```

- [ ] **Step 2: Run the body tests to verify they fail**

Run:

```bash
npm test -- src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx
```

Expected:

- FAIL because `StreamdownRenderer` still enables live animation during streaming.

- [ ] **Step 3: Update `StreamdownRenderer` and body mode switching**

Change `src/components/chat/shared/StreamdownRenderer.tsx` to expose explicit rendering mode:

```tsx
interface StreamdownRendererProps {
  children: string;
  isStreaming?: boolean;
  mode?: 'streaming-stable' | 'final';
}

export function StreamdownRenderer({ children, isStreaming, mode = 'final' }: StreamdownRendererProps) {
  const streamingStable = mode === 'streaming-stable';

  return (
    <Streamdown
      plugins={{ code, cjk }}
      isAnimating={false}
      parseIncompleteMarkdown={streamingStable}
      className={streamingStable ? 'streaming-stable-markdown' : undefined}
    >
      {children}
    </Streamdown>
  );
}
```

Then ensure `StreamingAgentBody` uses:

```tsx
<StreamdownRenderer
  isStreaming={isStreaming}
  mode={isStreaming ? 'streaming-stable' : 'final'}
>
  {agentText}
</StreamdownRenderer>
```

Do not re-enable streaming text animation in this task.

- [ ] **Step 4: Run the body tests to verify they pass**

Run:

```bash
npm test -- src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/chat/shared/StreamdownRenderer.tsx src/components/chat/streaming/StreamingAgentBody.tsx src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx
git commit -m "refactor: use stability-first markdown rendering while streaming"
```

## Task 6: Add the frame scheduler API and wire visible flushes to animation frames

**Files:**
- Modify: `src/stores/messageStore.ts`
- Modify: `src/components/chat/streaming/StreamingTurnRoot.tsx`
- Test: `src/__tests__/unit/stores/messageStore.test.ts`
- Test: `src/__tests__/unit/components/MessageList.test.tsx`

- [ ] **Step 1: Write the failing frame scheduling tests**

Add tests showing that many buffered deltas within one frame produce a single visible revision increment:

```ts
it('increments visible revision once for a single animation-frame flush', () => {
  const store = useMessageStore.getState();
  store.startStreaming('turn-1');
  store.startStreamingItem('thread-1', 'turn-1', { type: 'AgentMessage', id: 'a1', content: [] });

  store.bufferAgentContentDelta('a1', 'A');
  store.bufferAgentContentDelta('a1', 'B');
  store.bufferAgentContentDelta('a1', 'C');

  const before = useMessageStore.getState().streamingView?.revision ?? 0;
  store.flushVisibleStreaming();
  const after = useMessageStore.getState().streamingView?.revision ?? 0;

  expect(after - before).toBe(1);
});
```

Update `MessageList.test.tsx` to assert that multiple scheduler ingestions inside one frame produce one scroll reconcile.

- [ ] **Step 2: Run the relevant tests to verify they fail**

Run:

```bash
npm test -- src/__tests__/unit/stores/messageStore.test.ts src/__tests__/unit/components/MessageList.test.tsx
```

Expected:

- FAIL because frame scheduling is not wired end-to-end yet.

- [ ] **Step 3: Add scheduler wiring around `requestAnimationFrame`**

Implement a scheduler wrapper in `messageStore` or `StreamingTurnRoot` with this behavior:

```ts
const frameRef = useRef<number | null>(null);

const ensureStreamingFlushScheduled = useCallback(() => {
  if (frameRef.current !== null) return;

  frameRef.current = requestAnimationFrame(() => {
    frameRef.current = null;
    flushVisibleStreaming();
  });
}, [flushVisibleStreaming]);

useEffect(() => {
  if (!hasPendingStreamingBuffer) return;
  ensureStreamingFlushScheduled();
}, [hasPendingStreamingBuffer, ensureStreamingFlushScheduled]);
```

Requirements:

- only one frame may be pending at a time,
- completion events must still force a flush immediately,
- stale scheduled frames must be cancelled on unmount.

- [ ] **Step 4: Run the relevant tests to verify they pass**

Run:

```bash
npm test -- src/__tests__/unit/stores/messageStore.test.ts src/__tests__/unit/components/MessageList.test.tsx
```

Expected:

- PASS

- [ ] **Step 5: Commit**

```bash
git add src/stores/messageStore.ts src/components/chat/streaming/StreamingTurnRoot.tsx src/__tests__/unit/stores/messageStore.test.ts src/__tests__/unit/components/MessageList.test.tsx
git commit -m "feat: flush live streaming updates on animation frames"
```

## Task 7: Run focused regression verification and document residual issues

**Files:**
- Modify: none unless verification exposes a real bug
- Test: `src/__tests__/unit/stores/messageStore.test.ts`
- Test: `src/__tests__/unit/hooks/useCodexEvent.test.ts`
- Test: `src/__tests__/unit/hooks/useBottomLockScroll.test.ts`
- Test: `src/__tests__/unit/components/MessageList.test.tsx`
- Test: `src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx`

- [ ] **Step 1: Run the focused regression suite**

Run:

```bash
npm test -- \
  src/__tests__/unit/stores/messageStore.test.ts \
  src/__tests__/unit/hooks/useCodexEvent.test.ts \
  src/__tests__/unit/hooks/useBottomLockScroll.test.ts \
  src/__tests__/unit/components/MessageList.test.tsx \
  src/__tests__/unit/components/streaming/StreamingAgentBody.test.tsx
```

Expected:

- PASS

- [ ] **Step 2: Run type checking and capture unrelated failures separately**

Run:

```bash
npm run typecheck
```

Expected:

- May still fail because of pre-existing unrelated issues.
- If it fails, record the unrelated files in the final summary and do not fold those fixes into this task unless they block the streaming implementation directly.

- [ ] **Step 3: Commit**

```bash
git add .
git commit -m "test: verify streaming rendering redesign"
```

## Self-Review

### Spec coverage

- Buffer/view separation: Task 1
- Ingestion-only event handling: Task 2
- Single bottom-lock scroll owner: Task 3
- Split streaming UI islands: Task 4
- Stability-first Markdown during streaming: Task 5
- `requestAnimationFrame` frame-bounded visible flushes: Task 6
- Focused regression verification: Task 7

No spec section is currently uncovered.

### Placeholder scan

- Searched for `TODO`, `TBD`, `implement later`, `fill in details`, `similar to`, and vague test placeholders.
- No placeholders remain in this plan.

### Type consistency

- `streamingBuffer` and `streamingView` naming is consistent across Tasks 1, 2, and 6.
- `flushVisibleStreaming` is the only visible flush API name used throughout the plan.
- `useBottomLockScroll` owns `scheduleReconcile`, `attachContainer`, and `handleScroll` consistently across Tasks 3 and 6.

