import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useBottomLockScroll } from '@/hooks/useBottomLockScroll';

vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
  callback(0);
  return 1;
});

vi.stubGlobal('cancelAnimationFrame', vi.fn());

function makeScrollableElement({
  scrollHeight,
  clientHeight,
  onSetScrollTop,
}: {
  scrollHeight: number;
  clientHeight: number;
  onSetScrollTop: (value: number) => void;
}): HTMLDivElement {
  const element = document.createElement('div');
  let scrollTopValue = 0;

  Object.defineProperty(element, 'scrollHeight', {
    configurable: true,
    get: () => scrollHeight,
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

describe('useBottomLockScroll', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('writes scrollTop once after scheduleReconcile when bottomLock is true', () => {
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
});
