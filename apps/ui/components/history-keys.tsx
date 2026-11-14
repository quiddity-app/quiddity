"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef } from "react";
import { PlaceHistory, type Place } from "@/lib/history";
import { useView } from "./use-view";

/**
 * Back and forward through the main pane: Alt-Left and Alt-Right, or
 * Cmd/Ctrl-[ and Cmd/Ctrl-]. Both do the same thing. Only the main pane moves;
 * the side panel stays as it is, and opening or closing a card isn't a step
 * (`lib/history.ts`).
 *
 * The keys are caught before the editors and the webview see them, so the
 * webview's own history, which would replay the side panel, never runs.
 */
export function HistoryKeys() {
  const router = useRouter();
  const view = useView();
  const history = useRef(new PlaceHistory());
  const latest = useRef(view);
  latest.current = view;

  // Whatever the main pane shows is recorded, however it got there: a link,
  // search, or the browser's own back button. `/` only redirects to home.
  const { path, main } = view;
  useEffect(() => {
    if (path === "/" && !main) return;
    history.current.observe({ path, main });
  }, [path, main]);

  useEffect(() => {
    const go = (place: Place | undefined) => {
      if (!place) return;
      // Replace, not push: stepping through the history mustn't add to the browser's.
      router.replace(latest.current.href({ ...latest.current, ...place }));
    };
    const onKey = (e: KeyboardEvent) => {
      const alt = e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey;
      const mod = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey;
      const back = (alt && e.key === "ArrowLeft") || (mod && e.key === "[");
      const forward = (alt && e.key === "ArrowRight") || (mod && e.key === "]");
      if (!back && !forward) return;
      e.preventDefault();
      e.stopPropagation();
      go(back ? history.current.back() : history.current.forward());
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [router]);
  return null;
}
