"use client";

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { expandSelection, selectRange } from "@/lib/block-selection";
import { EditorView } from "@codemirror/view";
import { useGraph } from "@/lib/use-graph";
import type { Block, EntityId, ResolvedLink } from "@/lib/model";
import { EditingContext, useEditing, type Editing } from "./editing-context";
import { Outline } from "./entity-view";
import { AddRow } from "./ui";

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
    <AddRow
      empty={empty}
      emptyLabel="Empty. Click to add a block."
      label="+ Add a block"
      onAdd={() => {
        const id = editing.graph.createBlock(editing.entityId, undefined, "");
        editing.edit(id, "start");
      }}
    />
  );
}

/** Offered while an entity has no properties block: a first block to type
 * properties into. Until it holds one it is an ordinary empty block. */
function AddProperties() {
  const editing = useEditing()!;
  return (
    <AddRow
      empty={false}
      emptyLabel=""
      label="+ Add properties"
      onAdd={() => editing.addProperties()}
    />
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
  const scope = useId();
  // Blocks selected as blocks, and the one the selection grows from.
  const [selection, setSelection] = useState<{
    anchor: string;
    ids: ReadonlySet<string>;
  } | null>(null);
  // Blocks that have gone (an undo, another device) stop counting, and a
  // selection with none left is no selection.
  const selected = useMemo<ReadonlySet<string>>(() => {
    const here = new Set(order);
    return new Set([...(selection?.ids ?? [])].filter((id) => here.has(id)));
  }, [selection, order]);
  // Each block being edited, with its text from when editing began, so that
  // saving can tell the links it gained from the ones it had.
  const opened = useRef(new Map<string, string>());
  // Blocks made by the offer of a properties block, which go again if they are
  // left with nothing in them.
  const offered = useRef(new Set<string>());
  const save = useCallback(
    (id: string) => {
      const before = opened.current.get(id);
      opened.current.delete(id);
      if (before === undefined) return;
      try {
        graph.commitBlock(id, before);
        if (offered.current.delete(id)) graph.dropEmptyFirstBlock(id);
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
      selected,
      scope,
      edit: (id, caret) => {
        graph.settle();
        [...opened.current.keys()].filter((other) => other !== id).forEach(save);
        if (!opened.current.has(id)) opened.current.set(id, graph.blockText(id));
        setSelection(null);
        setCurrent({ id, caret });
      },
      stop: (id) => {
        graph.settle();
        save(id);
        setCurrent((c) => (c?.id === id ? null : c));
      },
      isOffered: (id) => offered.current.has(id),
      addProperties: () => {
        const id = graph.createBlock(entityId, 0, "");
        offered.current.add(id);
        graph.settle();
        [...opened.current.keys()].forEach(save);
        opened.current.set(id, graph.blockText(id));
        setSelection(null);
        setCurrent({ id, caret: "start" });
      },
    }),
    [graph, entityId, current, order, selected, scope, save],
  );
  // The handlers outlive a render, so they read the latest state from here.
  const live = useRef({ blocks, order, editing, selection: selected.size > 0 ? selection : null });
  live.current = { blocks, order, editing, selection: selected.size > 0 ? selection : null };
  useEffect(() => selectionHandlers(scope, live, setSelection), [scope]);
  return (
    <EditingContext.Provider value={editing}>
      <Outline blocks={blocks} links={links} />
      <AddBlock empty={blocks.length === 0} />
      {!blocks.some((b) => b.propertiesBlock) && !(blocks[0]?.text === "") && (
        <AddProperties />
      )}
    </EditingContext.Provider>
  );
}

/** The id of the block whose own row `target` is in, if it is one of `scope`'s. */
function blockOf(target: EventTarget | null, scope: string): string | null {
  const row = target instanceof Element ? target.closest<HTMLElement>("[data-block-id]") : null;
  return row && row.dataset.outline === scope ? (row.dataset.blockId ?? null) : null;
}

/**
 * The block under a height on the page: the last row that starts above it, so
 * dragging past either end of the outline, or off to the side, still reaches
 * the first or last block, and a gap between rows belongs to the one above.
 */
function blockAtHeight(scope: string, y: number): string | null {
  const rows = document.querySelectorAll<HTMLElement>(`[data-block-id][data-outline="${scope}"]`);
  let found: HTMLElement | undefined = rows[0];
  for (const row of rows) {
    if (row.getBoundingClientRect().top > y) break;
    found = row;
  }
  return found?.dataset.blockId ?? null;
}

/**
 * Selecting blocks as blocks, from the keyboard and by dragging. Document-wide
 * listeners, because a block being edited is a CodeMirror editor that would
 * take Select All for its own text, and a drag that starts in one block ends in
 * another. Each ignores what isn't in this outline.
 *
 * - Mod-A in an editor selects its text, as it always did. With the text
 *   selected, or in a block that isn't being edited, it selects the block, and
 *   each press after that takes in more (`expandSelection`).
 * - A drag from one block to another selects the blocks between them.
 * - Escape, or pressing anywhere, lets go.
 */
function selectionHandlers(
  scope: string,
  live: {
    current: {
      blocks: Block[];
      order: string[];
      editing: Editing;
      selection: { anchor: string; ids: ReadonlySet<string> } | null;
    };
  },
  setSelection: (selection: { anchor: string; ids: ReadonlySet<string> } | null) => void,
): () => void {
  const onKey = (e: KeyboardEvent) => {
    const { blocks, editing, selection } = live.current;
    if (e.key === "Escape" && selection && !editing.current) {
      setSelection(null);
      return;
    }
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== "a") return;
    // With blocks selected nothing has the focus; the body is where keys land.
    const anchor =
      blockOf(e.target, scope) ??
      (selection && e.target === document.body ? selection.anchor : null);
    if (anchor === null) return;
    // In an editor the first press is the editor's, selecting the block's text;
    // it is the next, with the text all selected (or none to select), that
    // goes on to the block.
    const view =
      e.target instanceof Element && e.target.closest(".cm-editor")
        ? EditorView.findFromDOM(e.target.closest<HTMLElement>(".cm-editor")!)
        : null;
    if (view) {
      const { from, to } = view.state.selection.main;
      if (from !== 0 || to !== view.state.doc.length) return;
    }
    e.preventDefault();
    // Before the editor, which would select its text.
    e.stopPropagation();
    const from = selection?.anchor ?? anchor;
    const next = expandSelection(blocks, from, selection?.ids ?? new Set());
    if (!next) return;
    if (editing.current) editing.stop(editing.current.id);
    setSelection({ anchor: from, ids: next });
  };

  let drag: { anchor: string; crossed: boolean } | null = null;
  let userSelect = "";
  const endDrag = () => {
    if (!drag) return;
    if (drag.crossed) {
      document.body.style.userSelect = userSelect;
      // The release would otherwise click the block it was over, and edit it.
      const swallow = (e: Event) => e.stopPropagation();
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, true));
    }
    drag = null;
  };
  const onDown = (e: PointerEvent) => {
    endDrag();
    if (e.button !== 0) return;
    if (live.current.selection) setSelection(null);
    // A finger drags to scroll.
    if (e.pointerType === "touch") return;
    const anchor = blockOf(e.target, scope);
    if (anchor === null || (e.target as Element).closest("a, button, input")) return;
    drag = { anchor, crossed: false };
  };
  const onMove = (e: PointerEvent) => {
    if (!drag) return;
    if (!(e.buttons & 1)) return endDrag();
    const over = blockAtHeight(scope, e.clientY);
    if (over === null || (!drag.crossed && over === drag.anchor)) return;
    if (!drag.crossed) {
      drag.crossed = true;
      userSelect = document.body.style.userSelect;
      document.body.style.userSelect = "none";
      const { editing } = live.current;
      if (editing.current) editing.stop(editing.current.id);
    }
    window.getSelection()?.removeAllRanges();
    setSelection({ anchor: drag.anchor, ids: selectRange(live.current.order, drag.anchor, over) });
  };
  document.addEventListener("keydown", onKey, true);
  document.addEventListener("pointerdown", onDown, true);
  document.addEventListener("pointermove", onMove);
  document.addEventListener("pointerup", endDrag);
  document.addEventListener("pointercancel", endDrag);
  return () => {
    endDrag();
    document.removeEventListener("keydown", onKey, true);
    document.removeEventListener("pointerdown", onDown, true);
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerup", endDrag);
    document.removeEventListener("pointercancel", endDrag);
  };
}
