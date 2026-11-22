"use client";

import { use, useDeferredValue, useMemo, useSyncExternalStore } from "react";
import { bootGraph, type Graph } from "./graph";

/** The graph; suspends until it has loaded. */
export function useGraph(): Graph {
  return use(bootGraph());
}

/**
 * Reads from the graph, and re-renders when it changes. The graph keeps each
 * answer until the next edit, so what this returns is stable between them.
 */
export function useRead<T>(read: (graph: Graph) => T): T {
  const graph = useGraph();
  useSyncExternalStore(graph.subscribe, graph.getRevision, graph.getRevision);
  return read(graph);
}

/**
 * `useRead` for what can lag behind an edit: the sidebar, the references under
 * a page. The read is made in a deferred render, which React does once what
 * the person did has been drawn and gives up on if they do something more, so
 * a read that takes a while (every entity's title, every block that mentions
 * this one) is never what a keystroke waits for. React doesn't defer for ever:
 * a render left that long is done regardless.
 *
 * `read` is made again when `deps` change as well as when the graph does, as
 * for `useMemo`: list whatever it uses that isn't the graph.
 */
export function useDeferredRead<T>(read: (graph: Graph) => T, deps: readonly unknown[]): T {
  const graph = useGraph();
  const revision = useSyncExternalStore(graph.subscribe, graph.getRevision, graph.getRevision);
  const deferred = useDeferredValue(revision);
  // `read` is the caller's, and made afresh each render: `deps` stand for it.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => read(graph), [graph, deferred, ...deps]);
}
