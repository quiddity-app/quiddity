"use client";

import Link from "next/link";
import { useSyncExternalStore } from "react";
import type { SyncState } from "@/lib/model";
import { useGraph } from "@/lib/use-graph";

/** What each state is called, and the colour of its dot. */
const states: Record<SyncState, { label: string; dot: string }> = {
  alone: { label: "No other devices to sync with", dot: "bg-sync-alone" },
  offline: { label: "Not connected to any other device", dot: "bg-sync-offline" },
  behind: { label: "Connected, but out of date", dot: "bg-sync-behind" },
  synced: { label: "Connected and up to date", dot: "bg-sync-synced" },
  present: { label: "Another device is here", dot: "bg-sync-present" },
};

/**
 * How this device stands with the others: a cloud with a coloured dot, to the
 * left of search. It links to the Devices page. Nothing is drawn where there
 * is nothing to sync with (the published site), which never says.
 */
export function SyncIndicator() {
  const graph = useGraph();
  const status = useSyncExternalStore(graph.subscribeToStatus, graph.getStatus, graph.getStatus);
  if (status === undefined) return null;
  const { label, dot } = states[status];
  return (
    <Link
      href="/devices/"
      aria-label={`Sync: ${label}`}
      title={label}
      data-state={status}
      className="relative flex h-8 w-8 items-center justify-center rounded text-ink-secondary hover:bg-surface-hover"
    >
      <svg
        aria-hidden="true"
        viewBox="0 0 24 24"
        className="h-5 w-5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M7 18a4 4 0 0 1-.6-7.96A5.5 5.5 0 0 1 17 8.5a4.75 4.75 0 0 1 0 9.5z" />
      </svg>
      <span
        aria-hidden="true"
        className={`absolute bottom-0.5 right-0.5 h-2.5 w-2.5 rounded-full ring-2 ring-dot-ring ${dot}`}
      />
    </Link>
  );
}
