'use client';

import { useEffect, useRef } from 'react';

import { useRouter } from 'next/navigation';

import { useView } from './use-view';

import { PlaceHistory, placeOpened, type Place } from '@/lib/history';

/**
 * Back and forward through the main pane: Alt-Left and Alt-Right, or
 * Cmd/Ctrl-[ and Cmd/Ctrl-]. Both do the same thing. Only the main pane moves;
 * the side panel stays as it is, and opening or closing a card isn't a step
 * (`lib/history.ts`).
 *
 * The keys are caught before the webview sees them, so the webview's own
 * history, which would replay the side panel, never runs. They are left alone
 * in an editor or a text field, where they are word movement and indent.
 */
const typing = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  (target.isContentEditable ||
    target.closest('input, textarea, [contenteditable]') !== null);

/**
 * One history for the page, started where the page was opened. The component
 * below mounts only once the URL can be read, which can be after a link has been
 * followed, and a history that begins at the place reached would have nowhere to
 * go back to. Opening the page is a place whether or not anything was
 * listening, so it is recorded as the module loads, before anything renders.
 */
const history = new PlaceHistory();
const opened = placeOpened();
if (opened) history.observe(opened);

export function HistoryKeys() {
  const router = useRouter();
  const view = useView();
  const latest = useRef(view);
  latest.current = view;

  // Whatever the main pane shows is recorded, however it got there: a link,
  // search, or the browser's own back button. `/` only redirects to home.
  const { path, main } = view;
  useEffect(() => {
    if (path === '/' && !main) return;
    history.observe({ path, main });
  }, [path, main]);

  useEffect(() => {
    const go = (place: Place | undefined) => {
      if (!place) return;
      // Replace, not push: stepping through the history mustn't add to the browser's.
      router.replace(latest.current.href({ ...latest.current, ...place }));
    };
    const onKey = (e: KeyboardEvent) => {
      if (typing(e.target)) return;
      const alt = e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey;
      const mod = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey;
      const back = (alt && e.key === 'ArrowLeft') || (mod && e.key === '[');
      const forward = (alt && e.key === 'ArrowRight') || (mod && e.key === ']');
      if (!back && !forward) return;
      e.preventDefault();
      e.stopPropagation();
      go(back ? history.back() : history.forward());
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [router]);
  return null;
}
