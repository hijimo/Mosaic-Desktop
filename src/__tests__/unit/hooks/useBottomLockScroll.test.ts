import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useBottomLockScroll } from '@/hooks/useBottomLockScroll';

let nextFrameId = 1;
const frameQueue = new Map<number, FrameRequestCallback>();

vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
  const id = nextFrameId++;
  frameQueue.set(id, callback);
  return id;
});

vi.stubGlobal('cancelAnimationFrame', vi.fn((id: number) => {
  frameQueue.delete(id);
}));
vi.stubGlobal(
  'ResizeObserver',
  class {
    observe = vi.fn();
    disconnect = vi.fn();
  },
);

function makeScrollableElement({
  scrollHeight,
  clientHeight,
  onSetScrollTop,
}: {
  scrollHeight: number | (() => number);
  clientHeight: number;
  onSetScrollTop: (value: number) => void;
}): HTMLDivElement {
  const element = document.createElement('div');
  let scrollTopValue = 0;

  Object.defineProperty(element, 'scrollHeight', {
    configurable: true,
    get: () => (typeof scrollHeight === 'function' ? scrollHeight() : scrollHeight),
  });

  Object.defineProperty(element, 'clientHeight', {
    configurable: true,
    get: () => clientHeight,
  });

  Object.defineProperty(element, 'scrollTop', {
    configurable: true,
    get: () => scrollTopValue,
    set: (value: number) => {
      scrollTopValue = value;
      onSetScrollTop(value);
    },
  });

  return element;
}

function runNextFrame(timestamp = 16): void {
  const iterator = frameQueue.entries().next();
  if (iterator.done) {
    throw new Error('No animation frame scheduled');
  }

  const [id, callback] = iterator.value;
  frameQueue.delete(id);
  callback(timestamp);
}

function runAllFrames(): void {
  while (frameQueue.size > 0) {
    runNextFrame();
  }
}

describe('useBottomLockScroll', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    frameQueue.clear();
    nextFrameId = 1;
  });

  it('animates scrollTop to bottom across animation frames', () => {
    const scrollTopSet = vi.fn();
    const container = makeScrollableElement({
      scrollHeight: 1200,
      clientHeight: 400,
      onSetScrollTop: scrollTopSet,
    });

    const { result } = renderHook(() => useBottomLockScroll());

    act(() => {
      result.current.attachContainer(container);
      result.current.setBottomLock(true);
      result.current.scheduleReconcile();
    });

    expect(scrollTopSet).not.toHaveBeenCalledWith(1200);

    act(() => {
      runNextFrame();
    });

    expect(scrollTopSet).toHaveBeenCalled();
    expect(scrollTopSet.mock.lastCall?.[0]).toBeLessThan(1200);

    act(() => {
      runAllFrames();
    });

    expect(scrollTopSet.mock.lastCall?.[0]).toBe(1200);
  });

  it('reconcileNow and scheduleReconcile do not double-write for same height', () => {
    const scrollTopSet = vi.fn();
    const container = makeScrollableElement({
      scrollHeight: 1200,
      clientHeight: 400,
      onSetScrollTop: scrollTopSet,
    });

    const { result } = renderHook(() => useBottomLockScroll());

    act(() => {
      result.current.attachContainer(container);
      result.current.setBottomLock(true);
      result.current.reconcileNow();
      result.current.scheduleReconcile();
      runAllFrames();
    });

    expect(scrollTopSet).toHaveBeenCalledTimes(1);
    expect(scrollTopSet).toHaveBeenCalledWith(1200);
  });

  it('does not write scrollTop when bottomLock is false', () => {
    const scrollTopSet = vi.fn();
    const container = makeScrollableElement({
      scrollHeight: 1200,
      clientHeight: 400,
      onSetScrollTop: scrollTopSet,
    });

    const { result } = renderHook(() => useBottomLockScroll());

    act(() => {
      result.current.attachContainer(container);
      result.current.setBottomLock(false);
      result.current.scheduleReconcile();
    });

    expect(scrollTopSet).not.toHaveBeenCalled();
  });

  it('cancels smooth auto scroll after user wheel interaction', () => {
    const scrollTopSet = vi.fn();
    const container = makeScrollableElement({
      scrollHeight: 1200,
      clientHeight: 400,
      onSetScrollTop: scrollTopSet,
    });

    const { result } = renderHook(() => useBottomLockScroll());

    act(() => {
      result.current.attachContainer(container);
      result.current.setBottomLock(true);
      result.current.scheduleReconcile();
      runNextFrame();
    });

    const writesBeforeWheel = scrollTopSet.mock.calls.length;

    act(() => {
      container.dispatchEvent(new WheelEvent('wheel', { deltaY: -60 }));
      runAllFrames();
    });

    expect(scrollTopSet.mock.calls.length).toBe(writesBeforeWheel);
    expect(cancelAnimationFrame).toHaveBeenCalled();
  });
});
