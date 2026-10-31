import {
  Annotation,
  EditorSelection,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
} from "@codemirror/state";
import { EditorView, keymap, showTooltip, type KeyBinding, type Tooltip } from "@codemirror/view";
import { fromGraph } from "./cm-annotations";
import type { TitleHit } from "./model";

/** How many pages the list offers. */
const MAX_HITS = 8;

/** A `[[link]]` in a document: the text between the brackets. */
export interface LinkSpan {
  from: number;
  to: number;
  /** Where the name ends and the qualifiers (`#key(value)`) start. */
  nameEnd: number;
}

const qualifierStart = /[\p{L}\p{N}_[]/u;

/** The links in one line of text, as offsets into it. A backslash escapes the
 * next character, as it does for the parser. */
export function linksIn(line: string): LinkSpan[] {
  const out: LinkSpan[] = [];
  let i = 0;
  while (i < line.length - 1) {
    if (line[i] === "\\") {
      i += 2;
    } else if (line[i] === "[" && line[i + 1] === "[") {
      const from = i + 2;
      let to = from;
      let nameEnd = -1;
      while (to < line.length && !(line[to] === "]" && line[to + 1] === "]")) {
        if (line[to] === "\\") to++;
        else if (nameEnd < 0 && line[to] === "#" && qualifierStart.test(line[to + 1] ?? "")) {
          nameEnd = to;
        }
        to++;
      }
      if (to >= line.length) {
        i = from; // Never closed, so not a link; there may be one inside.
        continue;
      }
      out.push({ from, to, nameEnd: nameEnd < 0 ? to : nameEnd });
      i = to + 2;
    } else {
      i++;
    }
  }
  return out;
}

/** The link whose text (or edges) the caret is in. */
export function linkAt(state: EditorState, pos: number): LinkSpan | null {
  const line = state.doc.lineAt(pos);
  const at = pos - line.from;
  const found = linksIn(line.text).find((l) => at >= l.from && at <= l.to);
  if (!found) return null;
  return {
    from: line.from + found.from,
    to: line.from + found.to,
    nameEnd: line.from + found.nameEnd,
  };
}

/** A list of pages offered for the link the caret is in. */
interface Open {
  /** The link's text, which choosing replaces. */
  from: number;
  to: number;
  /** What the user typed, which comes back if they leave the list. */
  typed: string;
  hits: TitleHit[];
  /** The entry the text was replaced by, or null while it is as typed. */
  selected: number | null;
}

/** Marks our own replacement of the link's text. */
const previewing = Annotation.define<boolean>();
const choose = StateEffect.define<number | null>();
const dismiss = StateEffect.define<null>();

export interface LinkCompletion {
  /** Pages whose titles match, best first. */
  search(query: string, limit: number): TitleHit[];
  /** Goes to the page a link reaches, making it if it goes nowhere. */
  follow(key: string): void;
}

/**
 * Completion for the page a `[[link]]` names.
 *
 * The list is offered when the link's text is typed in (or the link is new),
 * not when the caret merely arrives. Arrow keys or a click move into the list
 * and put the entry in the link, so what is written is what was chosen; Enter
 * accepts whatever is written and steps out of the link. Enter in a link that
 * was not being completed follows it.
 */
export function linkCompletion(options: LinkCompletion): Extension {
  const field = StateField.define<Open | null>({
    create: () => null,
    update(open, tr) {
      if (tr.effects.some((e) => e.is(dismiss))) return null;
      const span = linkAt(tr.state, tr.state.selection.main.head);
      if (!span) return null;

      if (tr.annotation(previewing)) {
        const picked = tr.effects.find((e) => e.is(choose));
        return open && { ...open, from: span.from, to: span.to, selected: picked?.value ?? null };
      }
      if (!tr.docChanged) return open && { ...open, from: span.from, to: span.to };

      // Typed in the link, or the link is new: the brackets are part of what
      // changed. Changes from elsewhere don't open a list.
      let touched = false;
      if (!tr.annotation(fromGraph)) {
        tr.changes.iterChangedRanges((_a, _b, from, to) => {
          if (from <= span.to + 2 && to >= span.from - 2) touched = true;
        });
      }
      if (!touched) return open && { ...open, from: span.from, to: span.to };
      if (tr.state.selection.main.head > span.nameEnd) return null;

      const typed = tr.state.doc.sliceString(span.from, span.nameEnd);
      const query = typed.trim();
      return {
        from: span.from,
        to: span.to,
        typed,
        hits: query ? options.search(query, MAX_HITS) : [],
        selected: null,
      };
    },
    provide: (f) => showTooltip.from(f, (open) => tooltip(open)),
  });

  /** Writes `hit`'s link (or the text as typed) in place of the link's text. */
  function pick(view: EditorView, open: Open, index: number | null) {
    const text = index === null ? open.typed : open.hits[index]!.link;
    view.dispatch({
      changes: { from: open.from, to: open.to, insert: text },
      selection: EditorSelection.cursor(open.from + text.length),
      annotations: previewing.of(true),
      effects: choose.of(index),
      scrollIntoView: true,
    });
  }

  function tooltip(open: Open | null): Tooltip | null {
    if (!open || open.hits.length === 0) return null;
    return {
      pos: open.from,
      above: false,
      create(view) {
        const dom = document.createElement("div");
        dom.className = "cm-link-completions";
        dom.setAttribute("role", "listbox");
        dom.setAttribute("aria-label", "Pages");
        open.hits.forEach((hit, index) => {
          dom.append(row(hit, index === open.selected, () => pick(view, open, index)));
        });
        return { dom };
      },
    };
  }

  const current = (view: EditorView) => view.state.field(field);

  /** Moves through the entries; past either end is back to what was typed. */
  const move = (delta: 1 | -1) => (view: EditorView) => {
    const open = current(view);
    if (!open || open.hits.length === 0) return false;
    const slots = open.hits.length + 1; // The entries, then "as typed".
    const at = open.selected ?? open.hits.length;
    const next = (at + delta + slots) % slots;
    pick(view, open, next === open.hits.length ? null : next);
    return true;
  };

  const keys: KeyBinding[] = [
    { key: "ArrowDown", run: move(1) },
    { key: "ArrowUp", run: move(-1) },
    {
      key: "Enter",
      run: (view) => {
        const { main } = view.state.selection;
        if (!main.empty) return false;
        const span = linkAt(view.state, main.head);
        if (!span) return false;
        if (current(view)) {
          // Accept what is written, and carry on after the link.
          view.dispatch({
            selection: EditorSelection.cursor(span.to + 2),
            effects: dismiss.of(null),
          });
        } else {
          options.follow(view.state.doc.sliceString(span.from, span.to));
        }
        return true;
      },
    },
    {
      // Puts back what was typed if an entry was chosen. Otherwise it is
      // the outliner's: leaving the block leaves the list.
      key: "Escape",
      run: (view) => {
        const open = current(view);
        if (!open || open.selected === null) return false;
        pick(view, open, null);
        view.dispatch({ effects: dismiss.of(null) });
        return true;
      },
    },
  ];

  return [
    field,
    keymap.of(keys),
    EditorView.theme({
      ".cm-tooltip.cm-link-completions": {
        backgroundColor: "Canvas",
        color: "CanvasText",
        border: "1px solid color-mix(in srgb, CanvasText 20%, transparent)",
        borderRadius: "0.375rem",
        boxShadow: "0 4px 12px rgb(0 0 0 / 0.15)",
        fontSize: "0.875rem",
        maxWidth: "min(32rem, 90vw)",
        padding: "0.25rem",
        overflow: "hidden",
      },
    }),
  ];
}

/** One entry: the title with what was matched marked, the qualifiers that
 * will be written, and the page's properties. Page text is untrusted, so it
 * only ever goes in as text. */
function row(hit: TitleHit, selected: boolean, onChoose: () => void): HTMLElement {
  const el = document.createElement("div");
  el.setAttribute("role", "option");
  el.setAttribute("aria-selected", String(selected));
  el.className = "cm-link-completion";
  el.style.cssText = `padding:0.125rem 0.5rem;border-radius:0.25rem;cursor:pointer;${
    selected ? "background:color-mix(in srgb, CanvasText 15%, transparent);" : ""
  }`;
  // Keeps the focus in the editor.
  el.addEventListener("mousedown", (e) => e.preventDefault());
  el.addEventListener("click", onChoose);

  const title = document.createElement("div");
  title.style.cssText = "white-space:nowrap;overflow:hidden;text-overflow:ellipsis;";
  let at = 0;
  const label = hit.entity.title;
  for (const [start, end] of hit.ranges) {
    if (start > at) title.append(label.slice(at, start));
    const mark = document.createElement("mark");
    mark.textContent = label.slice(start, end);
    mark.style.cssText = "background:transparent;color:inherit;font-weight:600;text-decoration:underline;";
    title.append(mark);
    at = end;
  }
  title.append(label.slice(at));
  if (hit.qualifiers) {
    const qualifiers = document.createElement("span");
    qualifiers.textContent = hit.qualifiers;
    qualifiers.className = "cm-link-completion-qualifiers";
    qualifiers.style.cssText = "opacity:0.7;margin-left:0.25rem;";
    title.append(qualifiers);
  }
  el.append(title);

  if (hit.properties.length) {
    const props = document.createElement("div");
    props.className = "cm-link-completion-properties";
    props.style.cssText =
      "opacity:0.6;font-size:0.75rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;";
    props.textContent = hit.properties
      .map((p) => (p.value === null ? `#${p.key}` : `${p.key}: ${p.value}`))
      .join(" · ");
    el.append(props);
  }
  return el;
}
