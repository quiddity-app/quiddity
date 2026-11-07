"use client";

import { useState } from "react";
import type { EntityId } from "@/lib/model";
import { useGraph, useRead } from "@/lib/use-graph";

/**
 * The heading of the entity in the main pane, which can be renamed in place:
 * Enter, or leaving the field, keeps the new name and Escape drops it. The old
 * name stays as an alias, so links written against it still work. Journals and
 * property entities are identified by their date or name, so theirs is just a
 * heading.
 */
export function EditableTitle({ id }: { id: EntityId }) {
  const graph = useGraph();
  const { title, renamable } = useRead((g) => g.entity(id));
  const [draft, setDraft] = useState<string | null>(null);
  if (!renamable) return <>{title}</>;

  const finish = (keep: boolean) => {
    const name = draft?.trim();
    setDraft(null);
    if (keep && name && name !== title) graph.renameEntity(id, name);
  };

  return draft === null ? (
    <button
      type="button"
      title="Rename"
      className="cursor-text text-left hover:underline hover:decoration-dotted"
      onClick={() => setDraft(title)}
    >
      {title}
    </button>
  ) : (
    <input
      autoFocus
      aria-label="Name"
      value={draft}
      className="w-full rounded bg-transparent outline outline-1 outline-edit-outline"
      onChange={(e) => setDraft(e.target.value)}
      onFocus={(e) => e.target.select()}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(true);
        else if (e.key === "Escape") finish(false);
      }}
    />
  );
}
