import { useCallback, useEffect, useRef, useState } from 'react';

const BOTTOM_THRESHOLD = 50;
const AUTO_SCROLL_MIN_STEP = 16;
const AUTO_SCROLL_EASING = 0.35;

export function useBottomLockScroll(): {
  attachContainer: (node: HTMLDivElement | null) => void;
  bottomLock: boolean;
  setBottomLock: (value: boolean) => void;
  handleScroll: () => void;
  reconcileNow: () => void;
  scheduleReconcile: () => void;
} {
  const [containerNode, setContainerNode] = useState<HTMLDivElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameRef = useRef<number | null>(null);
  const bottomLockRef = useRef(true);
  const lastAppliedScrollHeightRef = useRef<number | null>(null);
  const autoScrollTargetRef = useRef<number | null>(null);
  const programmaticScrollRef = useRef(false);
  const [bottomLock, setBottomLock] = useState(true);

  const updateBottomLock = useCallback((value: boolean) => {
    bottomLockRef.current = value;
    setBottomLock(value);
  }, []);

  const attachContainer = useCallback((node: HTMLDivElement | null) => {
    containerRef.current = node;
    setContainerNode(node);
  }, []);

  const cancelAutoScroll = useCallback(() => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
    autoScrollTargetRef.current = null;
  }, []);

  const handleUserAutoScrollInterruption = useCallback(() => {
    cancelAutoScroll();
  }, [cancelAutoScroll]);

  const reconcileNow = useCallback(() => {
    const el = containerRef.current;
    if (!el || !bottomLockRef.current) return;

    cancelAutoScroll();
    const nextScrollHeight = el.scrollHeight;
    if (lastAppliedScrollHeightRef.current === nextScrollHeight) return;

    lastAppliedScrollHeightRef.current = nextScrollHeight;
    if (el.scrollTop !== nextScrollHeight) {
      programmaticScrollRef.current = true;
      el.scrollTop = nextScrollHeight;
    }
  }, [cancelAutoScroll]);

  const performAutoScrollStep = useCallback(() => {
    frameRef.current = null;

    const el = containerRef.current;
    if (!el || !bottomLockRef.current) {
      cancelAutoScroll();
      return;
    }

    const target = autoScrollTargetRef.current ?? el.scrollHeight;
    const distance = target - el.scrollTop;

    if (distance <= 1) {
      lastAppliedScrollHeightRef.current = target;
      if (el.scrollTop !== target) {
        programmaticScrollRef.current = true;
        el.scrollTop = target;
      }
      autoScrollTargetRef.current = null;
      return;
    }

    const step = Math.min(
      distance,
      Math.max(AUTO_SCROLL_MIN_STEP, distance * AUTO_SCROLL_EASING),
    );

    programmaticScrollRef.current = true;
    el.scrollTop += step;
    frameRef.current = requestAnimationFrame(performAutoScrollStep);
  }, [cancelAutoScroll]);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;

    if (programmaticScrollRef.current) {
      programmaticScrollRef.current = false;
      return;
    }

    cancelAutoScroll();
    updateBottomLock(
      el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_THRESHOLD,
    );
  }, [cancelAutoScroll, updateBottomLock]);

  const scheduleReconcile = useCallback(() => {
    const el = containerRef.current;
    if (!el || !bottomLockRef.current) return;

    const nextScrollHeight = el.scrollHeight;
    if (
      frameRef.current === null &&
      lastAppliedScrollHeightRef.current === nextScrollHeight
    ) {
      return;
    }

    autoScrollTargetRef.current = nextScrollHeight;
    if (frameRef.current !== null) return;

    frameRef.current = requestAnimationFrame(performAutoScrollStep);
  }, [performAutoScrollStep]);

  useEffect(() => {
    if (bottomLock) {
      lastAppliedScrollHeightRef.current = null;
    }
  }, [bottomLock]);

  useEffect(() => {
    const el = containerNode;
    if (!el || typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver(() => {
      if (!bottomLockRef.current) return;
      scheduleReconcile();
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, [containerNode, scheduleReconcile]);

  useEffect(() => {
    const el = containerNode;
    if (!el) return;

    el.addEventListener('wheel', handleUserAutoScrollInterruption, {
      passive: true,
    });
    el.addEventListener('touchstart', handleUserAutoScrollInterruption, {
      passive: true,
    });
    el.addEventListener('pointerdown', handleUserAutoScrollInterruption);

    return () => {
      el.removeEventListener('wheel', handleUserAutoScrollInterruption);
      el.removeEventListener('touchstart', handleUserAutoScrollInterruption);
      el.removeEventListener('pointerdown', handleUserAutoScrollInterruption);
    };
  }, [containerNode, handleUserAutoScrollInterruption]);

  useEffect(() => () => {
    cancelAutoScroll();
  }, [cancelAutoScroll]);

  return {
    attachContainer,
    bottomLock,
    setBottomLock: updateBottomLock,
    handleScroll,
    reconcileNow,
    scheduleReconcile,
  };
}
