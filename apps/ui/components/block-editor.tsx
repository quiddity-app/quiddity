"use client";

import { defaultKeymap, insertNewline } from "@codemirror/commands";
import { EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap, type KeyBinding } from "@codemirror/view";
import { useEffect, useLayoutEffect, useRef, useSyncExternalStore } from "react";
import { useRouter } from "next/navigation";
import { autoBrackets } from "@/lib/auto-brackets";
import { fromGraph } from "@/lib/cm-annotations";
import { linkCompletion } from "@/lib/link-completion";
import { useEditing, type Caret, type Editing } from "./editing-context";
import { useView } from "./use-view";

const isHighSurrogate = (s: string, i: number) => {
  const c = s.charCodeAt(i);
  return c >= 0xd800 && c <= 0xdbff;
};

/** The smallest edit that turns `from` into `to`. */
function diff(from: string, to: string) {
  let start = 0;
  while (start < from.length && start < to.length && from[start] === to[start]) start++;
  if (start > 0 && isHighSurrogate(from, start - 1)) start--;
  let endFrom = from.length;
  let endTo = to.length;
  while (endFrom > start && endTo > start && from[endFrom - 1] === to[endTo - 1]) {
    endFrom--;
    endTo--;
  }
  if (endFrom < from.length && endFrom > 0 && isHighSurrogate(from, endFrom - 1)) {
    endFrom++;
    endTo++;
  }
  return { from: start, to: endFrom, insert: to.slice(start, endTo) };
}

/**
 * The outliner's keys. Each acts on the graph, then says where editing
 * continues; the blocks it moved are remounted by React, so the caret travels
 * as part of the state rather than in the editor.
 */
function outlinerKeys(id: string, latest: { current: Editing }): KeyBinding[] {
  const head = (view: EditorView) => view.state.selection.main.head;
  const collapsed = (view: EditorView) => view.state.selection.main.empty;
  const neighbour = (delta: -1 | 1) => {
    const { order } = latest.current;
    return order[order.indexOf(id) + delta];
  };
  const go = (to: string | undefined, caret: Caret) => {
    if (to === undefined) return false;
    latest.current.edit(to, caret);
    return true;
  };
  return [
    {
      key: "Enter",
      run: (view) => {
        if (!collapsed(view)) return false;
        const { graph, edit } = latest.current;
        // Enter in an empty block steps out a level, as in most outliners;
        // at the top level there is nowhere to go, so it splits as usual.
        if (view.state.doc.length === 0) {
          edit(id, 0);
          if (graph.outdent(id)) return true;
        }
        edit(graph.split(id, head(view)), "start");
        return true;
      },
    },
    { key: "Shift-Enter", run: insertNewline },
    {
      key: "Tab",
      run: (view) => {
        const { graph, edit } = latest.current;
        edit(id, head(view));
        graph.indent(id);
        return true;
      },
    },
    {
      key: "Shift-Tab",
      run: (view) => {
        const { graph, edit } = latest.current;
        edit(id, head(view));
        graph.outdent(id);
        return true;
      },
    },
    {
      key: "Backspace",
      run: (view) => {
        if (!collapsed(view) || head(view) !== 0) return false;
        const { graph, edit } = latest.current;
        const merged = graph.mergeIntoPrevious(id);
        if (merged) edit(merged.into, merged.at);
        return true;
      },
    },
    {
      key: "ArrowUp",
      run: (view) =>
        collapsed(view) && view.state.doc.lineAt(head(view)).number === 1
          ? go(neighbour(-1), "end")
          : false,
    },
    {
      key: "ArrowDown",
      run: (view) =>
        collapsed(view) &&
        view.state.doc.lineAt(head(view)).number === view.state.doc.lines
          ? go(neighbour(1), "start")
          : false,
    },
    {
      key: "ArrowLeft",
      run: (view) => (collapsed(view) && head(view) === 0 ? go(neighbour(-1), "end") : false),
    },
    {
      key: "ArrowRight",
      run: (view) =>
        collapsed(view) && head(view) === view.state.doc.length
          ? go(neighbour(1), "start")
          : false,
    },
    ...([-1, 1] as const).map((delta) => ({
      key: delta < 0 ? "Alt-ArrowUp" : "Alt-ArrowDown",
      run: (view: EditorView) => {
        const { graph, edit } = latest.current;
        edit(id, head(view));
        graph.moveSibling(id, delta);
        return true;
      },
    })),
    {
      key: "Escape",
      run: () => {
        latest.current.stop(id);
        return true;
      },
    },
    // Undo is the graph's, so it covers structure and other blocks too.
    { key: "Mod-z", run: () => (latest.current.graph.undo(), true) },
    { key: "Mod-Shift-z", run: () => (latest.current.graph.redo(), true) },
    { key: "Mod-y", run: () => (latest.current.graph.redo(), true) },
  ];
}

/**
 * A block's Markdown as a CodeMirror editor bound to the graph: what is typed
 * goes into the block's text as the splice that was typed, and changes that
 * arrive from elsewhere are applied to the editor as the smallest edit.
 */
export function BlockEditor({ id, caret }: { id: string; caret: Caret }) {
  const editing = useEditing()!;
  const { graph } = editing;
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const router = useRouter();
  const nav = useView();
  const go = useRef({ router, nav });
  go.current = { router, nav };
  // The key handlers outlive a render, so they read the latest state from here.
  const latest = useRef(editing);
  latest.current = editing;
  // Removing a focused editor fires a blur, which isn't the user leaving it.
  // React removes the DOM before it runs ordinary effects' cleanups, but after
  // layout effects', so this flag is set in time.
  const closing = useRef(false);
  useLayoutEffect(() => {
    closing.current = false;
    return () => {
      closing.current = true;
    };
  }, []);

  useEffect(() => {
    const doc = graph.blockText(id);
    const anchor =
      caret === "start" ? 0 : caret === "end" ? doc.length : Math.min(caret, doc.length);
    const editor = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc,
        selection: { anchor },
        extensions: [
          EditorView.lineWrapping,
          autoBrackets(),
          // Before the outliner's keys: in a link, arrows and Enter are the list's.
          Prec.highest(
            linkCompletion({
              search: (query, limit) => graph.searchTitles(query, limit),
              follow: (key) => {
                let target: string;
                try {
                  target = graph.followLink(key);
                } catch {
                  return; // Empty, or ambiguous (no page to choose yet).
                }
                go.current.router.push(go.current.nav.toMain(target));
              },
            }),
          ),
          Prec.highest(keymap.of(outlinerKeys(id, latest))),
          keymap.of(defaultKeymap),
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) return;
            if (update.transactions.some((t) => t.annotation(fromGraph))) return;
            const changes: [number, number, string][] = [];
            update.changes.iterChanges((from, to, _fromB, _toB, inserted) =>
              changes.push([from, to, inserted.toString()]),
            );
            // Each change is in terms of the document before them all, so
            // from the end backwards they stay valid.
            for (const [from, to, insert] of changes.reverse()) {
              graph.spliceText(id, from, to - from, insert);
            }
          }),
          EditorView.domEventHandlers({
            // Losing the window's focus isn't leaving the block.
            blur: () => {
              if (!closing.current && document.hasFocus()) latest.current.stop(id);
            },
          }),
          EditorView.theme({
            "&": { backgroundColor: "transparent", color: "inherit" },
            "&.cm-focused": { outline: "none" },
            ".cm-scroller": { fontFamily: "inherit", lineHeight: "inherit" },
            ".cm-content": { padding: "0", caretColor: "currentColor" },
            ".cm-line": { padding: "0" },
            // The defaults assume a light background.
            ".cm-cursor, .cm-dropCursor": { borderLeftColor: "currentColor" },
            "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
              background: "rgb(56 189 248 / 0.35)",
            },
          }),
        ],
      }),
    });
    view.current = editor;
    editor.focus();
    return () => {
      editor.destroy();
      view.current = null;
    };
    // The caret is only where editing starts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, id]);

  // Follow the graph. Compared with the graph's own text rather than what was
  // last rendered, which can lag a keystroke behind.
  const revision = useSyncExternalStore(graph.subscribe, graph.getRevision, graph.getRevision);
  useEffect(() => {
    const editor = view.current;
    if (!editor) return;
    let text: string;
    try {
      text = graph.blockText(id);
    } catch {
      return; // Deleted; this editor is about to go.
    }
    const now = editor.state.doc.toString();
    if (text === now) return;
    editor.dispatch({
      changes: diff(now, text),
      annotations: fromGraph.of(true),
    });
  }, [graph, id, revision]);

  return <div ref={host} className="whitespace-pre-wrap break-words" />;
}
