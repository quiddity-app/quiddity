'use client';

import { useState, useSyncExternalStore } from 'react';

import { Region, Skeleton } from './boundary';
import { EntityLink } from './entity-link';
import { states } from './sync-indicator';
import {
  Button,
  ErrorLine,
  linkClass,
  pageTitle,
  sectionTitle,
  subsectionTitle,
} from './ui';

import type { Check, Outcome, Stats } from '@/lib/model';

import { useGraph, useRead } from '@/lib/use-graph';

/** Virtual entities drawn at first, and added by "Show more". A graph can link to thousands of names. */
const ROWS = 100;

/** Each number, in the order shown, with what it counts. */
const counts: { key: keyof Stats; label: string; about: string }[] = [
  {
    key: 'blocks',
    label: 'Blocks',
    about: 'Every block, the root of each entity included.',
  },
  {
    key: 'entities',
    label: 'Entities',
    about: 'Those that have been written, merged duplicates not counted.',
  },
  {
    key: 'journals',
    label: 'Journal pages',
    about: "How many of the entities are a day's journal.",
  },
  {
    key: 'mergedDuplicates',
    label: 'Merged duplicates',
    about:
      'Entities made twice, such as a journal on two devices, then merged. Kept, and hidden.',
  },
  { key: 'blobs', label: 'Files', about: 'Blocks that hold a file.' },
  {
    key: 'links',
    label: 'Links to entities',
    about: 'From any block, once for each link.',
  },
  {
    key: 'linksToNowhere',
    label: 'Links to nowhere',
    about: 'To a name nobody has made: the links to virtual entities.',
  },
  {
    key: 'ambiguous',
    label: 'Ambiguous blocks',
    about: 'Blocks with a link that matches more than one entity.',
  },
  {
    key: 'orphans',
    label: 'Orphans',
    about:
      'Entities nothing links to and that link to nothing, journal pages apart.',
  },
];

function Numbers({ stats }: Readonly<{ stats: Stats }>) {
  return (
    <section aria-labelledby="numbers" className="mb-8">
      <h2 id="numbers" className={sectionTitle}>
        The graph
      </h2>
      <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1">
        {counts.map(({ key, label, about }) => (
          <div key={key} className="contents">
            <dt title={about}>{label}</dt>
            <dd className="tabular-nums">{stats[key] as number}</dd>
          </div>
        ))}
        <div className="contents">
          <dt title="Names that are linked to, or used as a property's key, that no entity has.">
            Virtual entities
          </dt>
          <dd className="tabular-nums">{stats.virtuals.length}</dd>
        </div>
      </dl>
    </section>
  );
}

function Virtuals({ stats }: Readonly<{ stats: Stats }>) {
  const [shown, setShown] = useState(ROWS);
  const th = 'px-3 py-1 coarse:py-3 font-medium';
  const numeric = 'px-3 py-1 coarse:py-3 text-right tabular-nums';
  return (
    <section aria-labelledby="virtuals" className="mb-8">
      <h2 id="virtuals" className={sectionTitle}>
        Virtual entities
      </h2>
      <p className="mb-2 text-ink-secondary">
        Names that are linked to, or used as a property’s key, and that no
        entity has. Nothing is stored for them: each exists while something
        refers to it.
      </p>
      {stats.virtuals.length === 0 ? (
        <p>None.</p>
      ) : (
        <>
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-border">
                <th scope="col" className={th}>
                  Name
                </th>
                <th scope="col" className={th}>
                  ID
                </th>
                <th scope="col" className={`${th} text-right`}>
                  Incoming links
                </th>
                <th scope="col" className={`${th} text-right`}>
                  Linked from
                </th>
                <th scope="col" className={`${th} text-right`}>
                  Used as a key
                </th>
              </tr>
            </thead>
            <tbody>
              {stats.virtuals.slice(0, shown).map((v) => (
                <tr key={v.id} className="border-b border-border">
                  <th scope="row" className="px-3 py-1 coarse:py-3 font-normal">
                    <EntityLink id={v.id} className={linkClass}>
                      {v.title}
                    </EntityLink>
                  </th>
                  <td className="px-3 py-1 coarse:py-3">
                    <code className="break-all font-mono text-sm">{v.id}</code>
                  </td>
                  <td className={numeric}>{v.links}</td>
                  <td className={numeric}>{v.linkedFrom}</td>
                  <td className={numeric}>{v.keyUses}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {shown < stats.virtuals.length ? (
            <p className="mt-3 flex items-center gap-3">
              <span className="text-ink-secondary">
                {shown} of {stats.virtuals.length}
              </span>
              <Button onClick={() => setShown((n) => n + ROWS)}>
                Show more
              </Button>
            </p>
          ) : null}
        </>
      )}
    </section>
  );
}

function Outcomes({
  title,
  outcomes,
}: Readonly<{ title: string; outcomes: Outcome[] }>) {
  return (
    <section className="mb-4">
      <h3 className={subsectionTitle}>{title}</h3>
      <ul className="space-y-2">
        {outcomes.map((outcome) => {
          const n = outcome.problems.length;
          return (
            <li key={outcome.name}>
              <p>
                <span
                  className={
                    n === 0 ? 'text-ink-secondary' : 'font-medium text-danger'
                  }
                >
                  {n === 0 ? 'Passed' : n === 1 ? '1 problem' : `${n} problems`}
                </span>
                {': '}
                {outcome.name}
              </p>
              {n > 0 ? (
                <ul className="ml-6 list-disc">
                  {outcome.problems.map((problem, i) => (
                    <li key={`${i}:${problem.message}`} className="break-words">
                      {problem.id ? (
                        <EntityLink id={problem.id} className={linkClass}>
                          {problem.message}
                        </EntityLink>
                      ) : (
                        problem.message
                      )}
                    </li>
                  ))}
                </ul>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/**
 * The check reads the whole graph again, so it is run when asked and not on
 * every edit, and says when what it found is out of date.
 */
function Consistency() {
  const graph = useGraph();
  const revision = useRead((g) => g.getRevision());
  const [ran, setRan] = useState<{ check: Check; revision: number } | null>(
    null,
  );
  const [failure, setFailure] = useState<string | null>(null);
  const faults =
    ran?.check.integrity.reduce((n, o) => n + o.problems.length, 0) ?? 0;
  return (
    <section aria-labelledby="consistency" className="mb-8">
      <h2 id="consistency" className={sectionTitle}>
        Consistency check
      </h2>
      <p className="mb-2 text-ink-secondary">
        Compares what Quiddity keeps in step with the graph (its list of blocks,
        its index of names and links) against a fresh reading of it, and looks
        for what the text says that doesn’t add up. It reads the whole graph, so
        it runs when asked.
      </p>
      <p className="mb-3 flex flex-wrap items-center gap-3">
        <Button
          onClick={() => {
            try {
              setRan({ check: graph.check(), revision: graph.getRevision() });
              setFailure(null);
            } catch (error) {
              setFailure(
                error instanceof Error ? error.message : String(error),
              );
            }
          }}
        >
          {ran ? 'Run again' : 'Run check'}
        </Button>
        {ran && ran.revision !== revision ? (
          <span className="text-ink-secondary">
            The graph has changed since this ran.
          </span>
        ) : null}
      </p>
      {failure ? <ErrorLine>{failure}</ErrorLine> : null}
      {ran ? (
        <div aria-live="polite">
          <p role="status" className="mb-3 font-medium">
            {faults === 0
              ? 'The graph agrees with itself.'
              : `${faults} ${faults === 1 ? 'fault' : 'faults'} found.`}
          </p>
          <Outcomes title="Integrity" outcomes={ran.check.integrity} />
          <Outcomes title="Content" outcomes={ran.check.content} />
        </div>
      ) : null}
    </section>
  );
}

/**
 * How this device stands with the others, spelt out: the indicator's state and
 * what it means, and the devices the graph lists. The published site's
 * backend can't say, and the page says so.
 */
function Sync() {
  const graph = useGraph();
  const status = useSyncExternalStore(
    graph.subscribeToStatus,
    graph.getStatus,
    graph.getStatus,
  );
  const devices = useRead((g) => g.devices());
  return (
    <section aria-labelledby="sync" className="mb-8">
      <h2 id="sync" className={sectionTitle}>
        Sync
      </h2>
      {status === undefined ? (
        <p className="text-ink-secondary">
          This build can’t sync, so it has no state to show.
        </p>
      ) : (
        <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1">
          <dt>State</dt>
          <dd data-state={status} className="flex items-center gap-2">
            <span
              aria-hidden="true"
              className={`h-2.5 w-2.5 rounded-full ${states[status].dot}`}
            />
            <span>
              <span className="capitalize">{status}</span>:{' '}
              {states[status].label}
            </span>
          </dd>
          <dt>Meaning</dt>
          <dd>{states[status].about}</dd>
          <dt title="Devices that have saved this graph, this one included.">
            Devices listed
          </dt>
          <dd className="tabular-nums">{devices.length}</dd>
        </dl>
      )}
    </section>
  );
}

function Counted() {
  const stats = useRead((g) => g.stats());
  return (
    <>
      <Numbers stats={stats} />
      <Virtuals stats={stats} />
    </>
  );
}

export function StatsAndDebug() {
  return (
    <article className="mx-auto max-w-measure-table px-6 py-8">
      <h1 className={pageTitle}>Stats and debug</h1>
      <Region
        pending={<Skeleton className="h-4 w-1/2" />}
        failed={(error) => <ErrorLine>{error.message}</ErrorLine>}
      >
        <Counted />
        <Sync />
        <Consistency />
      </Region>
    </article>
  );
}
