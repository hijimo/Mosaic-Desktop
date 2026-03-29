import { useCallback, useEffect, useRef, useState } from 'react';

const BOTTOM_THRESHOLD = 50;

export function useBottomLockScroll(): {
  attachContainer: (node: HTMLDivElement | null) => void;
  bottomLock: boolean;
  setBottomLock: (value: boolean) => void;
  handleScroll: () => void;
  scheduleReconcile: () => void;
} {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameRef = useRef<number | null>(null);
  const bottomLockRef = useRef(true);
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

  const scheduleReconcile = useCallback(() => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
    }

    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      const el = containerRef.current;
      if (!el || !bottomLockRef.current) return;
      el.scrollTop = el.scrollHeight;
    });
  }, []);

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
    scheduleReconcile,
  };
}
