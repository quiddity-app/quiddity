'use client';

import { useEffect, useRef } from 'react';

/**
 * What the pointer or focus is on, for the toast in the corner of the page, as
 * a browser shows a link's destination in its status bar. One at a time: a
 * link says so when it is entered and again when it is left.
 */
let current: string | null = null;
const listeners = new Set<() => void>();

function set(text: string | null) {
  if (current === text) {
    return;
  }
  current = text;
  for (const listener of listeners) {
    listener();
  }
}

export const linkHint = {
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },
  get: () => current,
  /** For a link that has its own handlers: say what it points at. */
  show: (text: string) => {
    set(text);
  },
  hide: () => {
    set(null);
  },
};

/**
 * Handlers that show `text` while the pointer or focus is on a link, and stop
 * if the link is removed meanwhile. `text` is read when the link is entered,
 * so it can cost something.
 */
export function useLinkHint(text: () => string | null) {
  const shown = useRef<string | null>(null);
  useEffect(
    () => () => {
      // Only our own: another link may have been entered since.
      if (shown.current !== null && linkHint.get() === shown.current) {
        linkHint.hide();
      }
    },
    [],
  );
  const show = () => {
    shown.current = text();
    if (shown.current !== null) {
      linkHint.show(shown.current);
    }
  };
  const hide = () => {
    if (shown.current !== null && linkHint.get() === shown.current) {
      linkHint.hide();
    }
    shown.current = null;
  };
  return {
    onMouseEnter: show,
    onMouseLeave: hide,
    onFocus: show,
    onBlur: hide,
  };
}
