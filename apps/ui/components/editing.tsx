'use client';

import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from 'react';

import { EditorView } from '@codemirror/view';

import { EditingContext, useEditing, type Editing } from './editing-context';
import { Outline } from './entity-view';
import { AddRow } from './ui';

import type { Block, EntityId, ResolvedLink } from '@/lib/model';

import { useGraph } from '@/lib/use-graph';
import { moveDirection } from '@/lib/move-keys';
import {
  expandSelection,
  lastSelected,
  selectRange,
} from '@/lib/block-selection';

function flatten(blocks: Block[], out: string[] = []): string[] {
  for (const block of blocks) {
    out.push(block.id);
    flatten(block.children, out);
  }
  return out;
}

function AddBlock({ empty }: Readonly<{ empty: boolean }>) {
  const editing = useEditing()!;
  return (
    <AddRow
      empty={empty}
      emptyLabel="Empty. Click to add a block."
      label="+ Add a block"
      onAdd={() => {
        const id = editing.graph.createBlock(editing.entityId, undefined, '');
        editing.edit(id, 'start');
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

/** "1 block" or "3 blocks". */
const plural = (n: number) => `${n} ${n === 1 ? 'block' : 'blocks'}`;

/** An entity's blocks, which can be edited in place. */
export function EditableOutline({
  entityId,
  blocks,
  links,
}: Readonly<{
  entityId: EntityId;
  blocks: Block[];
  links: Record<string, ResolvedLink>;
}>) {
  const graph = useGraph();
  const [current, setCurrent] = useState<Editing['current']>(null);
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
  // What a screen reader is told. The same words said twice have to differ to
  // be said again, so a count goes with them.
  const [announcement, setAnnouncement] = useState({ text: '', count: 0 });
  // When something was last said about an action, so the selection it leaves
  // behind isn't announced over the top of it.
  const lastAction = useRef(0);
  const announce = useCallback((text: string) => {
    lastAction.current = Date.now();
    setAnnouncement((a) => ({ text, count: a.count + 1 }));
  }, []);
  // How many blocks are selected, once it settles: a drag changes it with every
  // step, which would be said every time. One block is read out, since it is
  // the one being worked on.
  const selectedKey = [...selected].join(',');
  useEffect(() => {
    if (selectedKey === '') return;
    const timer = setTimeout(() => {
      if (Date.now() - lastAction.current < 1000) return;
      const ids = selectedKey.split(',');
      let text = `${plural(ids.length)} selected`;
      if (ids.length === 1) {
        try {
          const first = graph.blockText(ids[0]!).split('\n')[0]!.slice(0, 80);
          if (first !== '') text += `: ${first}`;
        } catch {
          // The block has gone; the count will do.
        }
      }
      setAnnouncement((a) => ({ text, count: a.count + 1 }));
    }, 300);
    return () => clearTimeout(timer);
  }, [graph, selectedKey]);
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
    () => () => [...opened.current.keys()].forEach((key) => save(key)),
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
      announce,
      edit: (id, caret) => {
        graph.settle();
        [...opened.current.keys()]
          .filter((other) => other !== id)
          .forEach((key) => save(key));
        if (!opened.current.has(id))
          opened.current.set(id, graph.blockText(id));
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
        const id = graph.createBlock(entityId, 0, '');
        offered.current.add(id);
        graph.settle();
        [...opened.current.keys()].forEach((key) => save(key));
        opened.current.set(id, graph.blockText(id));
        setSelection(null);
        setCurrent({ id, caret: 'start' });
      },
    }),
    [graph, entityId, current, order, selected, scope, announce, save],
  );
  // The handlers outlive a render, so they read the latest state from here.
  const live = useRef({
    blocks,
    order,
    editing,
    selection: selected.size > 0 ? selection : null,
  });
  live.current = {
    blocks,
    order,
    editing,
    selection: selected.size > 0 ? selection : null,
  };
  useEffect(() => selectionHandlers(scope, live, setSelection), [scope]);
  return (
    <EditingContext.Provider value={editing}>
      <Outline blocks={blocks} links={links} />
      <AddBlock empty={blocks.length === 0} />
      {!blocks.some((b) => b.propertiesBlock) && blocks[0]?.text !== '' && (
        <AddProperties />
      )}
      <div
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className="sr-only"
      >
        {announcement.text}
        {announcement.count % 2 === 0 ? '' : '\u00a0'}
      </div>
    </EditingContext.Provider>
  );
}

/** The id of the block whose own row `target` is in, if it is one of `scope`'s. */
function blockOf(target: EventTarget | null, scope: string): string | null {
  const row =
    target instanceof Element
      ? target.closest<HTMLElement>('[data-block-id]')
      : null;
  return row?.dataset.outline === scope ? (row.dataset.blockId ?? null) : null;
}

/**
 * The block under a height on the page: the last row that starts above it, so
 * dragging past either end of the outline, or off to the side, still reaches
 * the first or last block, and a gap between rows belongs to the one above.
 */
function blockAtHeight(scope: string, y: number): string | null {
  const rows = document.querySelectorAll<HTMLElement>(
    `[data-block-id][data-outline="${scope}"]`,
  );
  let found: HTMLElement | undefined = rows[0];
  for (const row of rows) {
    if (row.getBoundingClientRect().top > y) break;
    found = row;
  }
  return found?.dataset.blockId ?? null;
}

type BlockSelection = { anchor: string; ids: ReadonlySet<string> } | null;

/** What the handlers read when they run: the latest of everything, not what it was when they were attached. */
interface Live {
  current: {
    blocks: Block[];
    order: string[];
    editing: Editing;
    selection: BlockSelection;
  };
}

type SetSelection = (selection: BlockSelection) => void;

/** Whether a key press is the page's or this outline's, and not some other field's. */
function isForBlocks(e: Event, scope: string): boolean {
  return e.target === document.body || blockOf(e.target, scope) !== null;
}

/**
 * The keys that move blocks move what is selected or, with nothing selected,
 * the block that has the focus. An editor has the same keys itself, which keep
 * its caret, so it is left to.
 */
function moveBlocks(
  e: KeyboardEvent,
  direction: -1 | 1,
  live: Live,
  scope: string,
) {
  const { editing, selection } = live.current;
  if (e.target instanceof Element && e.target.closest('.cm-editor')) return;
  const own = blockOf(e.target, scope);
  if (selection ? e.target !== document.body && own === null : own === null)
    return;
  e.preventDefault();
  const moved = editing.graph.moveBlocks(
    selection ? [...editing.selected] : [own!],
    direction,
  );
  const way = direction < 0 ? 'up' : 'down';
  editing.announce(moved ? `Moved ${way}` : `Can't move ${way}`);
  // Moving can remount the row, which drops the focus.
  if (!selection) {
    requestAnimationFrame(() =>
      document
        .querySelector<HTMLElement>(
          `[data-block-id="${own}"][data-outline="${scope}"] > div > [data-block-text]`,
        )
        ?.focus(),
    );
  }
}

/** Backspace and Delete delete what is selected, which undo brings back. */
function deleteSelected(
  e: KeyboardEvent,
  live: Live,
  scope: string,
  setSelection: SetSelection,
) {
  if (!isForBlocks(e, scope)) return;
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  e.preventDefault();
  const { editing } = live.current;
  const ids = [...editing.selected];
  editing.graph.deleteBlocks(ids);
  setSelection(null);
  editing.announce(`${plural(ids.length)} deleted`);
}

/** Tab and Shift-Tab indent and outdent what is selected, as they do the block being edited. */
function indentSelected(e: KeyboardEvent, live: Live, scope: string) {
  if (!isForBlocks(e, scope)) return;
  e.preventDefault();
  const { graph, selected, announce } = live.current.editing;
  const ids = [...selected];
  const moved = e.shiftKey ? graph.outdentBlocks(ids) : graph.indentBlocks(ids);
  const [did, cant] = e.shiftKey
    ? ['Outdented', "Can't outdent"]
    : ['Indented', "Can't indent"];
  announce(moved ? did : cant);
}

/**
 * Whether Select All in this target should go on to the block. In an editor
 * the first press is the editor's, selecting the block's text; it is the next,
 * with the text all selected (or none to select), that goes on.
 */
function editorHasNoTextLeft(target: EventTarget | null): boolean {
  const host =
    target instanceof Element
      ? target.closest<HTMLElement>('.cm-editor')
      : null;
  const view = host ? EditorView.findFromDOM(host) : null;
  if (!view) return true;
  const { from, to } = view.state.selection.main;
  return from === 0 && to === view.state.doc.length;
}

function selectAll(
  e: KeyboardEvent,
  live: Live,
  scope: string,
  setSelection: SetSelection,
) {
  if (
    !(e.metaKey || e.ctrlKey) ||
    e.altKey ||
    e.shiftKey ||
    e.key.toLowerCase() !== 'a'
  )
    return;
  const { blocks, editing, selection } = live.current;
  // With blocks selected nothing has the focus; the body is where keys land.
  const anchor =
    blockOf(e.target, scope) ??
    (selection && e.target === document.body ? selection.anchor : null);
  if (anchor === null) return;
  if (!editorHasNoTextLeft(e.target)) return;
  e.preventDefault();
  // Before the editor, which would select its text.
  e.stopPropagation();
  const from = selection?.anchor ?? anchor;
  const next = expandSelection(blocks, from, selection?.ids ?? new Set());
  if (!next) return;
  if (editing.current) editing.stop(editing.current.id);
  setSelection({ anchor: from, ids: next });
}

function handleKey(
  e: KeyboardEvent,
  live: Live,
  scope: string,
  setSelection: SetSelection,
) {
  const { editing, selection } = live.current;
  if (e.key === 'Escape' && selection && !editing.current) {
    setSelection(null);
    editing.announce('Selection cleared');
    return;
  }
  const direction = moveDirection(e);
  if (direction !== null) {
    moveBlocks(e, direction, live, scope);
  } else if ((e.key === 'Backspace' || e.key === 'Delete') && selection) {
    deleteSelected(e, live, scope, setSelection);
  } else if (
    e.key === 'Tab' &&
    selection &&
    !e.metaKey &&
    !e.ctrlKey &&
    !e.altKey
  ) {
    indentSelected(e, live, scope);
  } else {
    selectAll(e, live, scope, setSelection);
  }
}

/** A drag from one block to another, which selects the blocks between them. */
function dragSelection(scope: string, live: Live, setSelection: SetSelection) {
  let drag: { anchor: string; crossed: boolean } | null = null;
  let userSelect = '';
  const end = () => {
    if (!drag) return;
    if (drag.crossed) {
      document.body.style.userSelect = userSelect;
      // The release would otherwise click the block it was over, and edit it.
      const swallow = (e: Event) => e.stopPropagation();
      window.addEventListener('click', swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener('click', swallow, true));
    }
    drag = null;
  };
  const down = (e: PointerEvent) => {
    end();
    if (e.button !== 0) return;
    if (live.current.selection) setSelection(null);
    // A finger drags to scroll.
    if (e.pointerType === 'touch') return;
    const anchor = blockOf(e.target, scope);
    if (anchor === null || (e.target as Element).closest('a, button, input'))
      return;
    drag = { anchor, crossed: false };
  };
  const move = (e: PointerEvent) => {
    if (!drag) return;
    if (!(e.buttons & 1)) return end();
    const over = blockAtHeight(scope, e.clientY);
    if (over === null || (!drag.crossed && over === drag.anchor)) return;
    if (!drag.crossed) {
      drag.crossed = true;
      userSelect = document.body.style.userSelect;
      document.body.style.userSelect = 'none';
      const { editing } = live.current;
      if (editing.current) editing.stop(editing.current.id);
    }
    window.getSelection()?.removeAllRanges();
    setSelection({
      anchor: drag.anchor,
      ids: selectRange(live.current.order, drag.anchor, over),
    });
  };
  return { down, move, end };
}

/** Paste goes after the last selected block or the one with the focus, and selects what it made. */
function pasteBlocks(
  e: ClipboardEvent,
  own: string | null,
  live: Live,
  setSelection: SetSelection,
) {
  const { blocks, editing, selection } = live.current;
  const after = selection ? lastSelected(blocks, editing.selected) : own;
  if (after === null) return;
  const text = e.clipboardData?.getData('text/plain') ?? '';
  e.preventDefault();
  const made = editing.graph.pasteBlocks(after, text);
  const [first] = made;
  if (first !== undefined) setSelection({ anchor: first, ids: new Set(made) });
  editing.announce(
    made.length > 0 ? `${plural(made.length)} pasted` : 'Nothing to paste',
  );
}

/**
 * Copy, cut and paste are the browser's events, not keys, so the menu's work
 * too and the clipboard is written in the user's own gesture.
 */
function handleClipboard(
  e: ClipboardEvent,
  live: Live,
  scope: string,
  setSelection: SetSelection,
) {
  const { editing, selection } = live.current;
  // Text being edited, or in a field, keeps its own.
  if (
    e.target instanceof Element &&
    e.target.closest('input, textarea, [contenteditable]')
  ) {
    return;
  }
  const own = blockOf(e.target, scope);
  if (e.target !== document.body && own === null) return;
  if (e.type === 'paste') {
    pasteBlocks(e, own, live, setSelection);
    return;
  }
  if (!selection) return;
  e.preventDefault();
  const ids = [...editing.selected];
  e.clipboardData?.setData('text/plain', editing.graph.copyBlocks(ids));
  const cut = e.type === 'cut';
  if (cut) {
    editing.graph.deleteBlocks(ids);
    setSelection(null);
  }
  editing.announce(`${plural(ids.length)} ${cut ? 'cut' : 'copied'}`);
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
 * - Backspace and Delete delete the selected blocks, and copy, cut and paste
 *   work on them, as a Markdown outline.
 * - Tab and Shift-Tab indent and outdent the selected blocks, and the keys
 *   that move a block (`lib/move-keys.ts`) move them, or the focused block.
 * - A drag from one block to another selects the blocks between them.
 * - Escape, or pressing anywhere, lets go.
 */
function selectionHandlers(
  scope: string,
  live: Live,
  setSelection: SetSelection,
): () => void {
  const onKey = (e: KeyboardEvent) => handleKey(e, live, scope, setSelection);
  const onClipboard = (e: ClipboardEvent) =>
    handleClipboard(e, live, scope, setSelection);
  const drag = dragSelection(scope, live, setSelection);
  document.addEventListener('copy', onClipboard);
  document.addEventListener('cut', onClipboard);
  document.addEventListener('paste', onClipboard);
  document.addEventListener('keydown', onKey, true);
  document.addEventListener('pointerdown', drag.down, true);
  document.addEventListener('pointermove', drag.move);
  document.addEventListener('pointerup', drag.end);
  document.addEventListener('pointercancel', drag.end);
  return () => {
    drag.end();
    document.removeEventListener('copy', onClipboard);
    document.removeEventListener('cut', onClipboard);
    document.removeEventListener('paste', onClipboard);
    document.removeEventListener('keydown', onKey, true);
    document.removeEventListener('pointerdown', drag.down, true);
    document.removeEventListener('pointermove', drag.move);
    document.removeEventListener('pointerup', drag.end);
    document.removeEventListener('pointercancel', drag.end);
  };
}
