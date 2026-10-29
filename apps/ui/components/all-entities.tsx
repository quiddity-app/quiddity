"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { useGraph, useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { EntityLink } from "./entity-link";
import { useView } from "./use-view";

function List() {
  const rows = useRead((g) => g.summaries());
  if (rows.length === 0) return <p>Nothing here yet.</p>;
  const numeric = "px-3 py-1 text-right tabular-nums";
  return (
    <table className="w-full border-collapse text-left">
      <thead>
        <tr className="border-b border-slate-300 dark:border-slate-600">
          <th scope="col" className="px-3 py-1 font-medium">
            Title
          </th>
          <th scope="col" className="px-3 py-1 font-medium">
            Tags
          </th>
          <th scope="col" className={`${numeric} font-medium`}>
            Words
          </th>
          <th scope="col" className={`${numeric} font-medium`}>
            Incoming links
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((e) => (
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
