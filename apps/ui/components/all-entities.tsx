"use client";

import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import type { Summary } from "@/lib/model";
import { useGraph, useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { EntityLink } from "./entity-link";
import { useView } from "./use-view";
import { Button, ErrorLine, Input, linkClass } from "./ui";

type Column = "title" | "tags" | "words" | "incoming" | "created" | "modified";

/** Rows drawn at first, and added each time the end of them is reached. A
 * large graph has thousands, and drawing them all makes every sort and every
 * edit wait on the browser's layout. */
const ROWS = 200;

const columns: { key: Column; label: string; numeric: boolean }[] = [
  { key: "title", label: "Title", numeric: false },
  { key: "tags", label: "Tags", numeric: false },
  { key: "words", label: "Words", numeric: true },
  { key: "incoming", label: "Incoming links", numeric: true },
  { key: "created", label: "Created", numeric: false },
  { key: "modified", label: "Last modified", numeric: false },
];

const when = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

function When({ seconds }: { seconds: number | null }) {
  if (seconds === null) return null;
  const date = new Date(seconds * 1000);
  return <time dateTime={date.toISOString()}>{when.format(date)}</time>;
}

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

function compare(column: Column, a: Summary, b: Summary): number {
  switch (column) {
    case "title":
      return collator.compare(a.title, b.title);
    case "tags":
      return collator.compare(a.tags.join(" "), b.tags.join(" "));
    case "created":
    case "modified":
      return (a[column] ?? 0) - (b[column] ?? 0);
    default:
      return a[column] - b[column];
  }
}

/** Whether a row has nothing to sort by in this column, so goes last. */
const unknown = (column: Column, row: Summary) =>
  (column === "created" || column === "modified") && row[column] === null;

function List() {
  const rows = useRead((g) => g.summaries());
  // Rows arrive by title, and the sort is stable, so ties stay in that order.
  const [sort, setSort] = useState<{ column: Column; descending: boolean }>({
    column: "title",
    descending: false,
  });
  const graph = useGraph();
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const everyone = useRef<HTMLInputElement>(null);
  const [shown, setShown] = useState(ROWS);
  const more = useRef<HTMLDivElement>(null);
  // Rows can go without the user's say (an undo, a sync), so only those still
  // here count as selected.
  const selected = rows.filter((r) => chosen.has(r.id));
  const choose = (ids: Iterable<string>) => {
    setChosen(new Set(ids));
    // The question was about those entities, not whichever are chosen now.
    setConfirming(false);
  };
  const toggle = (id: string) =>
    choose(chosen.has(id) ? selected.filter((r) => r.id !== id).map((r) => r.id) : [...selected.map((r) => r.id), id]);
  const all = selected.length === rows.length;
  const them = selected.length === 1 ? "it" : "them";
  useEffect(() => {
    if (everyone.current) everyone.current.indeterminate = selected.length > 0 && !all;
  });
  const hasMore = shown < rows.length;
  useEffect(() => {
    const el = more.current;
    if (!el) return;
    const watcher = new IntersectionObserver(
      ([entry]) => entry?.isIntersecting && setShown((n) => n + ROWS),
      { rootMargin: "400px" },
    );
    watcher.observe(el);
    return () => watcher.disconnect();
  }, [shown, hasMore]);
  if (rows.length === 0) return <p>Nothing here yet.</p>;
  const sorted = rows.toSorted((a, b) => {
    const gap = Number(unknown(sort.column, a)) - Number(unknown(sort.column, b));
    if (gap !== 0) return gap;
    const order = compare(sort.column, a, b);
    return sort.descending ? -order : order;
  });
  const sortBy = (column: Column) => {
    setShown(ROWS);
    setSort((s) => ({
      column,
      descending: s.column === column && !s.descending,
    }));
  };
  const numeric = "px-3 py-1 text-right tabular-nums";
  return (
    <>
      <div className="mb-3 flex min-h-8 items-center gap-3" role="toolbar" aria-label="Selected entities">
        {selected.length === 0 ? null : confirming ? (
          <div role="alertdialog" aria-label="Confirm deletion" className="flex flex-wrap items-center gap-3">
            <p>
              Delete {selected.length === 1 ? `“${selected[0]?.title}”` : `${selected.length} entities`} and
              everything in {them}? Links to {them} will point nowhere. Undo brings {them} back.
            </p>
            <Button
              variant="danger"
              onClick={() => {
                graph.deleteEntities(selected.map((r) => r.id));
                choose([]);
              }}
            >
              Delete
            </Button>
            <Button autoFocus onClick={() => setConfirming(false)}>
              Cancel
            </Button>
          </div>
        ) : (
          <>
            <span>{selected.length} selected</span>
            <Button onClick={() => setConfirming(true)}>Delete…</Button>
          </>
        )}
      </div>
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="border-b border-border">
            <th scope="col" className="w-8 px-3 py-1">
              <input
                ref={everyone}
                type="checkbox"
                aria-label="Select all"
                checked={all}
                onChange={() => choose(all ? [] : rows.map((r) => r.id))}
              />
            </th>
            {columns.map((c) => {
              const active = sort.column === c.key;
              return (
                <th
                  key={c.key}
                  scope="col"
                  aria-sort={active ? (sort.descending ? "descending" : "ascending") : undefined}
                  className={`px-3 py-1 font-medium ${c.numeric ? "text-right" : ""}`}
                >
                  <button
                    type="button"
                    onClick={() => sortBy(c.key)}
                    className="font-medium hover:underline"
                  >
                    {c.label}
                    <span aria-hidden="true">{active ? (sort.descending ? " ↓" : " ↑") : ""}</span>
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {sorted.slice(0, shown).map((e) => (
            <tr key={e.id} className="border-b border-border">
              <td className="px-3 py-1">
                <input
                  type="checkbox"
                  aria-label={`Select ${e.title}`}
                  checked={chosen.has(e.id)}
                  onChange={() => toggle(e.id)}
                />
              </td>
              <th scope="row" className="px-3 py-1 font-normal">
                <EntityLink
                  id={e.id}
                  className={linkClass}
                >
                  {e.title}
                </EntityLink>
              </th>
              <td className="px-3 py-1 text-ink-secondary">
                {e.tags.map((t) => `#${t}`).join(" ")}
              </td>
              <td className={numeric}>{e.words}</td>
              <td className={numeric}>{e.incoming}</td>
              <td className="px-3 py-1 whitespace-nowrap">
                <When seconds={e.created} />
              </td>
              <td className="px-3 py-1 whitespace-nowrap">
                <When seconds={e.modified} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {hasMore ? <div ref={more} className="py-4" aria-hidden="true" /> : null}
    </>
  );
}

function NewEntity() {
  const graph = useGraph();
  const router = useRouter();
  const view = useView();
  const [name, setName] = useState("");
  return (
    <form
      className="mb-6 flex gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        const trimmed = name.trim();
        if (!trimmed) return;
        router.push(view.toMain(graph.createEntity(trimmed)));
      }}
    >
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="New entity"
        aria-label="Name of a new entity"
        className="min-w-0 flex-1"
      />
      <Button type="submit">Create</Button>
    </form>
  );
}

export function AllEntities() {
  return (
    <article className="mx-auto max-w-6xl px-6 py-8">
      <h1 className="mb-4 text-2xl font-semibold">All entities</h1>
      <Region
        pending={<Skeleton className="h-4 w-1/2" />}
        failed={(error) => (
          <ErrorLine>{error.message}</ErrorLine>
        )}
      >
        <NewEntity />
        <List />
      </Region>
    </article>
  );
}
