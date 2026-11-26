'use client';

import { Suspense, useEffect, useState, useSyncExternalStore } from 'react';

import { Region, Skeleton } from './boundary';
import { Add, ThisDevice } from './devices';
import { NetworkLog } from './network-log';
import { states } from './sync-indicator';
import { ErrorLine, pageTitle, sectionTitle } from './ui';

import { chooseBackend } from '@/lib/graph';
import { useGraph, useRead } from '@/lib/use-graph';

/** What the device is doing while it waits, from the sync details. */
function Progress() {
  const graph = useGraph();
  const details = useSyncExternalStore(
    graph.subscribeToStatus,
    graph.getStatus,
    graph.getStatus,
  );
  if (details === undefined) {
    return null;
  }
  const { label, dot } = states[details.state];
  return (
    <section aria-labelledby="joining-progress" className="mb-6">
      <h2 id="joining-progress" className={sectionTitle}>
        What this device is doing
      </h2>
      <p className="flex items-center gap-2">
        <span
          aria-hidden="true"
          className={`h-2.5 w-2.5 rounded-full ${dot}`}
        />
        {label}
      </p>
      <NetworkLog details={details} showDevice={false} />
    </section>
  );
}

/** The screen itself, which goes when a graph arrives. */
function Waiting() {
  const blank = useRead((g) => g.isBlank());
  if (!blank) {
    return null;
  }
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="joining-title"
      className="fixed inset-0 z-50 overflow-auto bg-surface p-8 text-ink"
    >
      <div className="mx-auto max-w-2xl">
        <h1 id="joining-title" className={pageTitle}>
          Waiting for a graph
        </h1>
        <p className="mb-6">
          This device has no graph yet. It will take one from another device
          that has it, once the two are let in to each other. This goes when the
          graph arrives.
        </p>
        <Region
          pending={<Skeleton className="h-4 w-1/2" />}
          failed={(error) => <ErrorLine>{error.message}</ErrorLine>}
        >
          <ThisDevice />
          <Add />
          <Progress />
        </Region>
      </div>
    </div>
  );
}

/**
 * Stands in for the graph on a device that chose to join one and has not been
 * given it: the page has nothing to show, and the way on is to let the other
 * device in. It draws nothing once the graph arrives, on a device that has one,
 * or outside the app.
 *
 * This device is let in for now, not for good: if the app closes before the
 * graph arrives, the other device has to be added again.
 */
export function Joining() {
  const [joining, setJoining] = useState(false);

  useEffect(() => {
    let current = true;
    void (async () => {
      try {
        const chosen = await chooseBackend();
        if (current && (await chosen.joining?.())) {
          setJoining(true);
        }
      } catch (error) {
        console.info("Couldn't ask whether this device is joining", error);
      }
    })();
    return () => {
      current = false;
    };
  }, []);

  if (!joining) {
    return null;
  }
  return (
    <Suspense fallback={null}>
      <Waiting />
    </Suspense>
  );
}
