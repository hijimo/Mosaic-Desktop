import { useCallback, useEffect, useRef, useState } from 'react';

const BOTTOM_THRESHOLD = 50;

export function useBottomLockScroll(): {
  attachContainer: (node: HTMLDivElement | null) => void;
  bottomLock: boolean;
  setBottomLock: (value: boolean) => void;
  handleScroll: () => void;
  reconcileNow: () => void;
  scheduleReconcile: () => void;
} {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameRef = useRef<number | null>(null);
  const bottomLockRef = useRef(true);
  const lastAppliedScrollHeightRef = useRef<number | null>(null);
  const [bottomLock, setBottomLock] = useState(true);

  const updateBottomLock = useCallback((value: boolean) => {
    bottomLockRef.current = value;
    setBottomLock(value);
  }, []);

  const attachContainer = useCallback((node: HTMLDivElement | null) => {
    containerRef.current = node;
  }, []);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    updateBottomLock(
      el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_THRESHOLD,
    );
  }, [updateBottomLock]);

  const reconcileNow = useCallback(() => {
    const el = containerRef.current;
    if (!el || !bottomLockRef.current) return;

    const nextScrollHeight = el.scrollHeight;
    if (lastAppliedScrollHeightRef.current === nextScrollHeight) return;

    lastAppliedScrollHeightRef.current = nextScrollHeight;
    if (el.scrollTop !== nextScrollHeight) {
      el.scrollTop = el.scrollHeight;
    }
  }, []);

  const scheduleReconcile = useCallback(() => {
    if (frameRef.current !== null) return;

    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      reconcileNow();
    });
  }, [reconcileNow]);

  useEffect(() => {
    if (bottomLock) {
      lastAppliedScrollHeightRef.current = null;
    }
  }, [bottomLock]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el || typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver(() => {
      if (!bottomLockRef.current) return;
      scheduleReconcile();
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, [scheduleReconcile]);

  useEffect(() => () => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
    }
  }, []);

  return {
    attachContainer,
    bottomLock,
    setBottomLock: updateBottomLock,
    handleScroll,
    reconcileNow,
    scheduleReconcile,
  };
}
