'use client';

import { useEffect, useRef, useState } from 'react';

import { Button, ErrorLine, pageTitle, sectionTitle } from './ui';

import type { Begin } from '@/lib/recovery';

import { chooseBackend, type Backend } from '@/lib/graph';

/**
 * The first thing a device with no graph shows: whether to start a graph of
 * its own or to join one that another device has. It is a choice, and not a
 * default, because a graph of its own never merges with another's: a second
 * device that started its own could never be given the first's.
 *
 * It covers the page, as `Recovery` does, and draws nothing on a device that
 * has a graph, or outside the app.
 */
export function Welcome() {
  const [backend, setBackend] = useState<Backend | null>(null);
  const [working, setWorking] = useState<Begin | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let current = true;
    void (async () => {
      try {
        const chosen = await chooseBackend();
        if (current && (await chosen.firstRun?.())) {
          setBackend(chosen);
        }
      } catch (error) {
        // Not being able to ask is not a reason to cover the page.
        console.info("Couldn't ask whether this is a first run", error);
      }
    })();
    return () => {
      current = false;
    };
  }, []);

  useEffect(() => {
    // Joining is first, and what a stray Enter takes: it can't make a graph
    // that won't merge.
    if (backend) {
      dialog.current?.querySelector<HTMLButtonElement>('button')?.focus();
    }
  }, [backend]);

  if (!backend) {
    return null;
  }
  const go = (how: Begin) => {
    setWorking(how);
    setFailure(null);
    // What the page already tried to load is gone with it: start the page over.
    backend
      .beginGraph?.(how)
      .then(() => {
        window.location.reload();
      })
      .catch((error) => {
        setFailure(String(error instanceof Error ? error.message : error));
        setWorking(null);
      });
  };
  return (
    <div
      ref={dialog}
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-title"
      className="fixed inset-0 z-50 overflow-auto bg-surface p-8 text-ink"
    >
      <div className="mx-auto max-w-2xl">
        <h1 id="welcome-title" className={pageTitle}>
          Welcome to Quiddity
        </h1>
        <p className="mb-6">
          This device has no graph yet. Is this your first device, or do you
          already use Quiddity somewhere else?
        </p>
        {failure ? <ErrorLine className="mb-4">{failure}</ErrorLine> : null}
        <section aria-labelledby="welcome-join" className="mb-6">
          <h2 id="welcome-join" className={sectionTitle}>
            Join a graph from another device
          </h2>
          <p className="mb-2">
            Starts empty, and waits for a device that has your graph. You will
            let the two devices in to each other, and the graph syncs here.
          </p>
          <Button
            onClick={() => {
              go('join');
            }}
            disabled={working !== null}
          >
            {working === 'join' ? 'Starting…' : 'Join a graph'}
          </Button>
        </section>
        <section aria-labelledby="welcome-new" className="mb-6">
          <h2 id="welcome-new" className={sectionTitle}>
            Start a new graph
          </h2>
          <p className="mb-2">
            For your first device. A graph started here will not merge with one
            on another device, so don’t choose this to add a device to a graph
            you already have.
          </p>
          <Button
            onClick={() => {
              go('new');
            }}
            disabled={working !== null}
          >
            {working === 'new' ? 'Starting…' : 'Start a new graph'}
          </Button>
        </section>
      </div>
    </div>
  );
}
