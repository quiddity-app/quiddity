'use client';

import { useEffect, useMemo, useState } from 'react';

import {
  ErrorLine,
  Input,
  linkClass,
  Page,
  pageTitle,
  sectionTitle,
} from './ui';

import { openExternal } from '@/lib/open-external';

/** One dependency, as `scripts/licenses.mjs` writes it. `texts` index into `Notices.texts`. */
interface Pkg {
  ecosystem: 'rust' | 'npm';
  name: string;
  version: string;
  license: string;
  url?: string;
  texts: number[];
}

/** `licenses.json`: everything Quiddity is built from, and the licence texts it must carry. */
interface Notices {
  packages: Pkg[];
  texts: string[];
}

const SOURCE = 'https://github.com/quiddity-app/quiddity';

/**
 * What the app is, who made it and what it is built from. The open source
 * licences ask for their text to travel with what they cover, so this is
 * where they are, whatever the platform: the desktop's About menu item and
 * the three-dot menu on every platform lead here (docs/Licences.md).
 */
export function About() {
  const [notices, setNotices] = useState<Notices | null>(null);
  const [failed, setFailed] = useState(false);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    let current = true;
    const url = `${process.env.NEXT_PUBLIC_QUIDDITY_BASE_PATH}/licenses.json`;
    fetch(url)
      .then((response) => {
        if (!response.ok) {
          throw new Error(String(response.status));
        }
        return response.json() as Promise<Notices>;
      })
      .then((loaded) => {
        if (current) {
          setNotices(loaded);
        }
      })
      .catch(() => {
        if (current) {
          setFailed(true);
        }
      });
    return () => {
      current = false;
    };
  }, []);

  const shown = useMemo(() => {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    return (notices?.packages ?? []).filter((p) =>
      words.every((w) => `${p.name} ${p.license}`.toLowerCase().includes(w)),
    );
  }, [notices, filter]);

  return (
    <Page width="text">
      <h1 className={pageTitle}>About Quiddity</h1>
      <p className="mb-2">
        Quiddity is free software under the Apache License 2.0. Its source is at{' '}
        <a
          href={SOURCE}
          className={linkClass}
          onClick={(e) => {
            e.preventDefault();
            void openExternal(SOURCE);
          }}
        >
          github.com/quiddity-app/quiddity
        </a>
        .
      </p>
      <p className="mb-8">
        It is built on the work of the people behind the open source software
        below, and is grateful to them. Each part is under its own licence,
        shown here as the licence asks.
      </p>
      <section aria-labelledby="built-from">
        <h2 id="built-from" className={sectionTitle}>
          Built from
        </h2>
        {failed && (
          <ErrorLine>
            The list of what Quiddity is built from couldn’t be loaded.
          </ErrorLine>
        )}
        {notices === null && !failed && (
          <p className="text-ink-muted">Loading…</p>
        )}
        {notices !== null && (
          <>
            <p className="mb-2 flex flex-wrap items-center gap-3">
              <label htmlFor="about-filter">Filter</label>
              <Input
                id="about-filter"
                type="search"
                value={filter}
                placeholder="Name or licence"
                className="min-w-0 flex-1"
                onChange={(e) => {
                  setFilter(e.target.value);
                }}
              />
            </p>
            <p className="mb-2 text-sm text-ink-muted" aria-live="polite">
              {shown.length} of {notices.packages.length} packages
            </p>
            <ul>
              {shown.map((p) => (
                <Row
                  key={`${p.ecosystem}:${p.name}@${p.version}`}
                  pkg={p}
                  texts={notices.texts}
                />
              ))}
            </ul>
          </>
        )}
      </section>
    </Page>
  );
}

/**
 * A package, closed until asked for: there are hundreds, and their texts
 * together are over a megabyte, so only an open one has its text in the page.
 */
function Row({ pkg, texts }: Readonly<{ pkg: Pkg; texts: string[] }>) {
  const [open, setOpen] = useState(false);
  return (
    <li className="border-b border-border py-1">
      <button
        type="button"
        aria-expanded={open}
        className="flex w-full flex-wrap items-baseline gap-x-3 rounded px-1 text-left hover:bg-surface-hover coarse:py-2"
        onClick={() => {
          setOpen((o) => !o);
        }}
      >
        <span className="font-medium">{pkg.name}</span>
        <span className="text-sm text-ink-muted">{pkg.version}</span>
        <span className="ml-auto text-sm text-ink-secondary">
          {pkg.license}
        </span>
      </button>
      {open && (
        <div className="px-1 py-2">
          {pkg.url !== undefined && (
            <p className="mb-2 break-all text-sm">
              <a
                href={pkg.url}
                className={linkClass}
                onClick={(e) => {
                  e.preventDefault();
                  void openExternal(pkg.url ?? '');
                }}
              >
                {pkg.url}
              </a>
            </p>
          )}
          {pkg.texts.map((t) => (
            <pre
              key={t}
              className="mb-2 overflow-x-auto whitespace-pre-wrap font-mono text-xs"
            >
              {texts[t]}
            </pre>
          ))}
        </div>
      )}
    </li>
  );
}
