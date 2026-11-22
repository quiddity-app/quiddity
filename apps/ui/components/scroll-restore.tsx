'use client';

import { useLayoutEffect } from 'react';

import { useView } from './use-view';

import { placeOpened } from '@/lib/history';
import { ScrollMemory } from '@/lib/scroll-memory';

/**
 * How long a page may go without changing before it is given up on as too short
 * to scroll back to where it was, and the longest it is waited for in all, in
 * case something keeps changing it (an animation, a feed).
 */
const QUIET_MS = 1000;
const GIVE_UP_MS = 5000;

/** What the person does that means the scroll position is theirs again. */
const TAKEOVER = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;

const memory = new ScrollMemory();
/** Where the pane is being put back to, while that is going on. */
let restoring: number | null = null;

// Remembering starts where the page was opened and hears the pane from the
// first, not from when the component below mounts: that waits for the URL, and
// the page can be scrolled and followed away from before then, which would
// leave the place it was scrolled on unknown. `scroll` doesn't bubble, so it is
// heard on the way down.
if (typeof document !== 'undefined') {
  const opened = placeOpened();
  if (opened) memory.arrive(opened);
  document.addEventListener(
    'scroll',
    ({ target }) => {
      if (
        !(target instanceof HTMLElement) ||
        target !== document.querySelector('main')
      )
        return;
      // Something other than the person moved it: a short page clamping it, or
      // the router scrolling a new route to the top. Put it back.
      if (restoring === null) memory.scrolled(target.scrollTop);
      else if (target.scrollTop !== restoring) target.scrollTop = restoring;
    },
    { capture: true, passive: true },
  );
}

/**
 * Puts the main pane back where it was left when it returns to a page, and at
 * the top for one it hasn't shown before (`lib/scroll-memory.ts`). The browser
 * can't: what scrolls is `main`, not the window, and the page's content is
 * swapped in place rather than loaded, so there is nothing for it to restore.
 *
 * A page that has only just been shown is usually still loading and too short
 * to scroll to where it was, so the position is applied again as it grows,
 * until the person scrolls for themselves, the page has been quiet for
 * `QUIET_MS`, or `GIVE_UP_MS` has passed. While
 * that is going on, scroll events are the clamping of a short page, or the
 * router scrolling a new route to the top, and not the person's: they are
 * undone, and aren't remembered.
 */
export function ScrollRestore() {
  const { path, main } = useView();

  // `/` only redirects to home, and shows no page of its own to remember.
  const home = path === '/' && !main;
  useLayoutEffect(() => {
    const pane = document.querySelector('main');
    if (!pane || home) return;
    const target = memory.arrive({ path, main });
    const apply = () => {
      pane.scrollTop = target;
    };
    apply();
    if (target === 0) return;

    restoring = target;
    const watcher = new MutationObserver(() => {
      apply();
      // Still loading, so wait longer.
      clearTimeout(quiet);
      quiet = setTimeout(stop, QUIET_MS);
    });
    // A suspended page is revealed by an attribute (`display`), not by being added.
    watcher.observe(pane, { childList: true, subtree: true, attributes: true });
    let quiet = setTimeout(stop, QUIET_MS);
    const cap = setTimeout(stop, GIVE_UP_MS);
    for (const type of TAKEOVER)
      window.addEventListener(type, stop, { capture: true, passive: true });
    function stop() {
      restoring = null;
      watcher.disconnect();
      clearTimeout(quiet);
      clearTimeout(cap);
      for (const type of TAKEOVER)
        window.removeEventListener(type, stop, { capture: true });
    }
    return stop;
  }, [path, main, home]);

  return null;
}
