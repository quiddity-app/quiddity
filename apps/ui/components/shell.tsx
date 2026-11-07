"use client";

import { useState, type ReactNode } from "react";
import { iconButtonClass } from "./ui";

/**
 * Three panes, with an action strip above the main one. The shell itself knows nothing about the URL, so it can render
 * (and be prerendered) without waiting on anything; each pane is handed in
 * already wrapped in its own boundary.
 *
 * Below `md` the left pane is a drawer; below `lg` the right pane overlays the
 * main one. The right `aside` is `empty:hidden`, so it takes no room until
 * something is open.
 */
export function Shell({
  left,
  right,
  actions,
  children,
}: {
  left: ReactNode;
  right: ReactNode;
  /** The strip's controls, at its right-hand end. */
  actions: ReactNode;
  children: ReactNode;
}) {
  const [drawer, setDrawer] = useState(false);
  const pane = "bg-surface-pane";
  return (
    <div className="flex h-dvh overflow-hidden">
      <aside
        className={`${pane} fixed inset-y-0 left-0 z-(--z-drawer) flex w-64 shrink-0 flex-col border-r border-border motion-safe:transition-transform md:static md:translate-x-0 ${
          drawer ? "translate-x-0" : "-translate-x-full"
        }`}
        onClick={() => setDrawer(false)}
      >
        <div className="px-4 py-3 text-lg font-semibold">Quiddity</div>
        {left}
      </aside>
      {drawer && (
        <button
          type="button"
          aria-label="Close navigation"
          className="fixed inset-0 z-(--z-scrim) bg-scrim md:hidden"
          onClick={() => setDrawer(false)}
        />
      )}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-3 py-2">
          <button
            type="button"
            aria-label="Open navigation"
            className={`${iconButtonClass} md:hidden`}
            onClick={() => setDrawer(true)}
          >
            ☰
          </button>
          <span className="font-semibold md:hidden">Quiddity</span>
          <div role="toolbar" aria-label="Actions" className="ml-auto flex items-center gap-2">
            {actions}
          </div>
        </div>
        <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      </div>
      <aside
        className={`${pane} fixed inset-y-0 right-0 z-(--z-pane) flex w-[min(100%,24rem)] shrink-0 flex-col border-l border-border shadow-overlay empty:hidden lg:static lg:w-96 lg:shadow-none`}
      >
        {right}
      </aside>
    </div>
  );
}
