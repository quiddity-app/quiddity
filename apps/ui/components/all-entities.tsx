"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import type { Summary } from "@/lib/model";
import { useGraph, useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { EntityLink } from "./entity-link";
import { useView } from "./use-view";

type Column = "title" | "tags" | "words" | "incoming";

const columns: { key: Column; label: string; numeric: boolean }[] = [
  { key: "title", label: "Title", numeric: false },
  { key: "tags", label: "Tags", numeric: false },
  { key: "words", label: "Words", numeric: true },
  { key: "incoming", label: "Incoming links", numeric: true },
];

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

function compare(column: Column, a: Summary, b: Summary): number {
  switch (column) {
    case "title":
      return collator.compare(a.title, b.title);
    case "tags":
      return collator.compare(a.tags.join(" "), b.tags.join(" "));
    default:
      return a[column] - b[column];
  }
}

function List() {
  const rows = useRead((g) => g.summaries());
  // Rows arrive by title, and the sort is stable, so ties stay in that order.
  const [sort, setSort] = useState<{ column: Column; descending: boolean }>({
    column: "title",
    descending: false,
  });
  if (rows.length === 0) return <p>Nothing here yet.</p>;
  const sorted = rows.toSorted((a, b) => {
    const order = compare(sort.column, a, b);
    return sort.descending ? -order : order;
  });
  const sortBy = (column: Column) =>
    setSort((s) => ({
      column,
      descending: s.column === column && !s.descending,
    }));
  const numeric = "px-3 py-1 text-right tabular-nums";
  return (
    <table className="w-full border-collapse text-left">
      <thead>
        <tr className="border-b border-slate-300 dark:border-slate-600">
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
        {sorted.map((e) => (
          <tr key={e.id} className="border-b border-slate-200 dark:border-slate-700">
            <th scope="row" className="px-3 py-1 font-normal">
              <EntityLink
                id={e.id}
                className="text-sky-700 underline-offset-2 hover:underline dark:text-sky-300"
              >
                {e.title}
              </EntityLink>
            </th>
            <td className="px-3 py-1 text-slate-600 dark:text-slate-400">
              {e.tags.map((t) => `#${t}`).join(" ")}
            </td>
            <td className={numeric}>{e.words}</td>
            <td className={numeric}>{e.incoming}</td>
          </tr>
        ))}
      </tbody>
    </table>
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
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="New entity"
        aria-label="Name of a new entity"
        className="min-w-0 flex-1 rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-600"
      />
      <button
        type="submit"
        className="rounded border border-slate-300 px-3 py-1 hover:bg-slate-200 dark:border-slate-600 dark:hover:bg-slate-700"
      >
        Create
      </button>
    </form>
  );
}

export function AllEntities() {
  return (
    <article className="mx-auto max-w-4xl px-6 py-8">
      <h1 className="mb-4 text-2xl font-semibold">All entities</h1>
      <Region
        pending={<Skeleton className="h-4 w-1/2" />}
        failed={(error) => (
          <p role="alert" className="text-red-700 dark:text-red-300">
            {error.message}
          </p>
        )}
      >
        <NewEntity />
        <List />
      </Region>
    </article>
  );
}
