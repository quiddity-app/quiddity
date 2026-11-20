"use client";

import { useLayoutEffect, useRef } from "react";
import { ScrollMemory } from "@/lib/scroll-memory";
import { useView } from "./use-view";

/**
 * How long a page may go without changing before it is given up on as too short
 * to scroll back to where it was, and the longest it is waited for in all, in
 * case something keeps changing it (an animation, a feed).
 */
const QUIET_MS = 1000;
const GIVE_UP_MS = 5000;

/** What the person does that means the scroll position is theirs again. */
const TAKEOVER = ["wheel", "touchstart", "pointerdown", "keydown"] as const;

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
  const memory = useRef(new ScrollMemory());
  /** Where the pane is being put back to, while that is going on. */
  const restoring = useRef<number | null>(null);

  useLayoutEffect(() => {
    const pane = document.querySelector("main");
    if (!pane) return;
    const onScroll = () => {
      const target = restoring.current;
      // Something other than the person moved it: a short page clamping it, or
      // the router scrolling a new route to the top. Put it back.
      if (target === null) memory.current.scrolled(pane.scrollTop);
      else if (pane.scrollTop !== target) pane.scrollTop = target;
    };
    pane.addEventListener("scroll", onScroll, { passive: true });
    return () => pane.removeEventListener("scroll", onScroll);
  }, []);

  // `/` only redirects to home, and shows no page of its own to remember.
  const home = path === "/" && !main;
  useLayoutEffect(() => {
    const pane = document.querySelector("main");
    if (!pane || home) return;
    const target = memory.current.arrive({ path, main });
    const apply = () => {
      pane.scrollTop = target;
    };
    apply();
    if (target === 0) return;

    restoring.current = target;
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
    for (const type of TAKEOVER) window.addEventListener(type, stop, { capture: true, passive: true });
    function stop() {
      restoring.current = null;
      watcher.disconnect();
      clearTimeout(quiet);
      clearTimeout(cap);
      for (const type of TAKEOVER) window.removeEventListener(type, stop, { capture: true });
    }
    return stop;
  }, [path, main, home]);

  return null;
}
