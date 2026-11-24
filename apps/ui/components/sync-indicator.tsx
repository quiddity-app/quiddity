'use client';

import {
  useEffect,
  useId,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';

import Link from 'next/link';

import { linkClass } from './ui';

import type { SyncDetails, SyncState } from '@/lib/model';

import { useGraph } from '@/lib/use-graph';

/**
 * What each state is called, the colour of its dot, and what it means (for
 * the Stats and debug page, which spells it out).
 */
export const states: Record<
  SyncState,
  { label: string; dot: string; about: string }
> = {
  alone: {
    label: 'No other devices to sync with',
    dot: 'bg-sync-alone',
    about: 'No other device is listed in the graph, and none is connected.',
  },
  offline: {
    label: 'Not connected to any other device',
    dot: 'bg-sync-offline',
    about: 'Other devices are listed in the graph, and none is connected.',
  },
  behind: {
    label: 'Connected, but out of date',
    dot: 'bg-sync-behind',
    about:
      'Connected, but a device is known to have something this one lacks, or typing is being held back until someone can see it.',
  },
  synced: {
    label: 'Connected and up to date',
    dot: 'bg-sync-synced',
    about: 'Connected, and nothing is outstanding either way.',
  },
  present: {
    label: 'Another device is here',
    dot: 'bg-sync-present',
    about: 'Up to date, and another device has a block open.',
  },
};

/** An ID short enough to read; the whole of it is the tooltip. */
function ShortId({ id }: Readonly<{ id: string }>) {
  return (
    <code title={id} className="font-mono text-sm">
      {id.length > 12 ? `${id.slice(0, 8)}…` : id}
    </code>
  );
}

/**
 * What the state rests on: who is connected, how many devices the graph
 * lists, and whether typing is being held back. Shared by the pop-up and the
 * Stats and debug page.
 */
export function SyncFacts({ details }: Readonly<{ details: SyncDetails }>) {
  const { connected, listed, held, watching } = details;
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
      <dt>Connected</dt>
      <dd className="tabular-nums">
        {connected.length === 0 ? (
          'None'
        ) : (
          <ul>
            {connected.map(({ device, ahead }) => (
              <li key={device}>
                <ShortId id={device} />
                {ahead ? (
                  <span className="text-ink-secondary">
                    {' '}
                    has more than this
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </dd>
      <dt title="Other devices the graph lists, connected or not.">Listed</dt>
      <dd className="tabular-nums">{listed}</dd>
      <dt title="Blocks typed in that no other device can see yet.">
        Typing held back
      </dt>
      <dd className="tabular-nums">{held}</dd>
      <dt title="Devices with a block open.">Here now</dt>
      <dd className="tabular-nums">{watching}</dd>
    </dl>
  );
}

/**
 * How this device stands with the others: a cloud with a coloured dot, to the
 * left of search. Pressing it opens the details, with a way on to the Devices
 * page. Nothing is drawn where there is nothing to sync with (the published
 * site), which never says.
 */
export function SyncIndicator() {
  const graph = useGraph();
  const details = useSyncExternalStore(
    graph.subscribeToStatus,
    graph.getStatus,
    graph.getStatus,
  );
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();

  useEffect(() => {
    if (!open) {
      return;
    }
    const onPointerDown = (e: PointerEvent) => {
      if (!(e.target instanceof Node)) {
        return;
      }
      if (
        !panel.current?.contains(e.target) &&
        !button.current?.contains(e.target)
      ) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpen(false);
        button.current?.focus();
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (details === undefined) {
    return null;
  }
  const { state } = details;
  const { label, dot, about } = states[state];
  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        aria-label={`Sync: ${label}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? `${id}-details` : undefined}
        title={label}
        data-state={state}
        onClick={() => {
          setOpen((o) => !o);
        }}
        className="relative flex h-8 w-8 coarse:h-11 coarse:w-11 items-center justify-center rounded text-ink-secondary hover:bg-surface-hover"
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
      </button>
      {open && (
        <div
          ref={panel}
          id={`${id}-details`}
          role="dialog"
          aria-label="Sync details"
          className="absolute right-0 top-full z-(--z-overlay) mt-1 w-72 rounded-lg border border-border bg-surface p-3 text-sm shadow-overlay"
        >
          <p className="mb-1 flex items-center gap-2 font-medium">
            <span
              aria-hidden="true"
              className={`h-2.5 w-2.5 rounded-full ${dot}`}
            />
            {label}
          </p>
          <p className="mb-3 text-ink-secondary">{about}</p>
          <SyncFacts details={details} />
          <p className="mt-3">
            <Link
              href="/devices/"
              className={linkClass}
              onClick={() => {
                setOpen(false);
              }}
            >
              Devices
            </Link>
          </p>
        </div>
      )}
    </div>
  );
}
