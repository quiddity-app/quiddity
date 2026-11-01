"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useGraph } from "@/lib/use-graph";
import type { Block, EntityId, ResolvedLink } from "@/lib/model";
import { EditingContext, useEditing, type Editing } from "./editing-context";
import { Outline } from "./entity-view";

function flatten(blocks: Block[], out: string[] = []): string[] {
  for (const block of blocks) {
    out.push(block.id);
    flatten(block.children, out);
  }
  return out;
}

function AddBlock({ empty }: { empty: boolean }) {
  const editing = useEditing()!;
  return (
    <button
      type="button"
      className={`mt-1 block w-full rounded px-2 py-1 text-left text-sm text-slate-500 hover:bg-slate-200 dark:text-slate-400 dark:hover:bg-slate-700 ${
        empty ? "" : "opacity-0 focus:opacity-100 hover:opacity-100"
      }`}
      onClick={() => {
        const id = editing.graph.createBlock(editing.entityId, undefined, "");
        editing.edit(id, "start");
      }}
    >
      {empty ? "Empty. Click to add a block." : "+ Add a block"}
    </button>
  );
}

/** An entity's blocks, which can be edited in place. */
export function EditableOutline({
  entityId,
  blocks,
  links,
}: {
  entityId: EntityId;
  blocks: Block[];
  links: Record<string, ResolvedLink>;
}) {
  const graph = useGraph();
  const [current, setCurrent] = useState<Editing["current"]>(null);
  const order = useMemo(() => flatten(blocks), [blocks]);
  // Each block being edited, with its text from when editing began, so that
  // saving can tell the links it gained from the ones it had.
  const opened = useRef(new Map<string, string>());
  const save = useCallback(
    (id: string) => {
      const before = opened.current.get(id);
      opened.current.delete(id);
      if (before === undefined) return;
      try {
        graph.commitBlock(id, before);
      } catch {
        // The block went in an undo, so there is nothing to save.
      }
    },
    [graph],
  );
  // Other devices are told which block is open.
  const open = current?.id;
  useEffect(() => {
    if (open === undefined) return;
    graph.focus(open);
    return () => graph.blur(open);
  }, [graph, open]);
  // Leaving the page is leaving the block.
  useEffect(
    () => () => [...opened.current.keys()].forEach(save),
    [entityId, save],
  );
  const editing = useMemo<Editing>(
    () => ({
      graph,
      entityId,
      current,
      order,
      edit: (id, caret) => {
        graph.settle();
        [...opened.current.keys()].filter((other) => other !== id).forEach(save);
        if (!opened.current.has(id)) opened.current.set(id, graph.blockText(id));
        setCurrent({ id, caret });
      },
      stop: (id) => {
        graph.settle();
        save(id);
        setCurrent((c) => (c?.id === id ? null : c));
      },
    }),
    [graph, entityId, current, order, save],
  );
  return (
    <EditingContext.Provider value={editing}>
      <Outline blocks={blocks} links={links} />
      <AddBlock empty={blocks.length === 0} />
    </EditingContext.Provider>
  );
}
