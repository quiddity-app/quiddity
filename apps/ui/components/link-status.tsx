'use client';

import { useSyncExternalStore } from 'react';

import { linkHint } from '@/lib/link-hint';

/**
 * The toast in the bottom-left corner that names where a link goes while the
 * pointer or focus is on it, as a browser's status bar does. It is
 * `aria-hidden`, since the link has its own name, and takes no pointer events
 * so it never covers what it is about.
 */
export function LinkStatus() {
  const text = useSyncExternalStore(
    linkHint.subscribe,
    linkHint.get,
    () => null,
  );
  if (text === null) {
    return null;
  }
  return (
    <div
      aria-hidden="true"
      data-link-status
      className="pointer-events-none fixed bottom-0 left-0 z-(--z-overlay) max-w-[min(100%,40rem)] truncate rounded-tr border-t border-r border-border bg-surface px-2 py-0.5 text-xs text-ink-secondary"
    >
      {text}
    </div>
  );
}
