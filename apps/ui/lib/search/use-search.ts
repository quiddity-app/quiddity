'use client';

import {
  useDeferredValue,
  useEffect,
  useState,
  useSyncExternalStore,
} from 'react';

import { useGraph } from '../use-graph';

import type { Facet, Hit } from './facet';

export interface Section {
  facet: Facet;
  hits: Hit[];
}

/**
 * Runs every facet on `query` and keeps each one's latest answer, so the
 * panel fills in as facets finish and an old answer stays up (rather than
 * flickering empty) until its replacement arrives. Searches again when the
 * graph changes. Empty sections are kept; the panel decides what to show.
 *
 * What is typed goes into the box at once: the searching is for the deferred
 * query, which React gets to once the keystroke has been drawn and drops for
 * a newer one if more has been typed since. Each facet then runs in a task of
 * its own, so a fast one is drawn before a slow one starts, and a query that
 * has been replaced by then never starts the slow ones at all. Searching is
 * the replica's, in this thread, so what it saves is the work not done.
 */
export function useSearch(facets: Facet[], query: string): Section[] {
  const graph = useGraph();
  const revision = useSyncExternalStore(
    graph.subscribe,
    graph.getRevision,
    graph.getRevision,
  );
  const [answers, setAnswers] = useState<Record<string, Hit[]>>({});
  const trimmed = query.trim();
  const deferred = useDeferredValue(trimmed);

  useEffect(() => {
    if (!deferred) {
      setAnswers({});
      return;
    }
    // Set when the query or the graph has moved on: an answer that arrives
    // then is for a question nobody is asking.
    let stale = false;
    const timers = facets.map((facet) =>
      setTimeout(() => {
        void Promise.resolve()
          .then(() => facet.search(deferred, graph))
          .catch((error: unknown) => {
            console.error(`Search of ${facet.id} failed`, error);
            return [];
          })
          .then((hits) => {
            if (!stale) {
              setAnswers((old) => ({ ...old, [facet.id]: hits }));
            }
          });
      }, 0),
    );
    return () => {
      stale = true;
      for (const timer of timers) {
        clearTimeout(timer);
      }
    };
  }, [facets, deferred, graph, revision]);

  return facets.map((facet) => ({
    facet,
    hits: trimmed ? (answers[facet.id] ?? []) : [],
  }));
}
