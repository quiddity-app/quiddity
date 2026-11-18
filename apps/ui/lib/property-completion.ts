import {
  acceptCompletion,
  autocompletion,
  closeCompletion,
  completionStatus,
  moveCompletionSelection,
  startCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import type { KeyHit, ValueHit } from "./model";

/** How many entries the list offers. */
const MAX_HITS = 8;

/** What the list is made from. */
export interface PropertySource {
  keys(query: string, limit: number): KeyHit[];
  values(key: string, query: string, limit: number): ValueHit[];
  /** Whether to offer anything in this block: only an entity's properties block. */
  active(): boolean;
}

/** A line that is already a key and its value: `key:: partial`. A key can have
 * spaces, so it is everything before the first `::`. */
const withValue = /^(\s*)([^\s:#[\]()][^:]*?)::([ \t]*)(.*)$/;

/** A line that is only the start of a key. A `#` is a tag and a `[` a link,
 * which have their own completion. */
const startOfKey = /^(\s*)([^\s:#[\]()][^:]*)$/;

/**
 * Completion in an entity's properties block: the keys the graph already uses
 * where a key is being typed, and the values already given to the key after
 * its `::`. Nothing is offered anywhere else, or in a block that isn't one.
 * Ctrl-Space asks for the list before a character has been typed.
 */
export function propertyCompletion(source: PropertySource): Extension {
  return [
    completion(source),
    // The library's own keys, except Escape: while a completion is only
    // pending (a moment after typing, before any list) its Escape would close
    // nothing and still take the key, so leaving the block would need two.
    keymap.of([
      { key: "Enter", run: acceptCompletion },
      { key: "ArrowDown", run: moveCompletionSelection(true) },
      { key: "ArrowUp", run: moveCompletionSelection(false) },
      { key: "PageDown", run: moveCompletionSelection(true, "page") },
      { key: "PageUp", run: moveCompletionSelection(false, "page") },
      { key: "Ctrl-Space", run: startCompletion },
      {
        key: "Escape",
        run: (view) => (completionStatus(view.state) === "active" ? closeCompletion(view) : false),
      },
    ]),
  ];
}

function useCount(uses: number): string {
  if (uses === 0) return "defined";
  return `${uses} ${uses === 1 ? "use" : "uses"}`;
}

function completion(source: PropertySource): Extension {
  return autocompletion({
    defaultKeymap: false,
    override: [(context) => complete(context, source)],
    icons: false,
    // The default ignores keys for 75 ms after the list opens, so a quick Enter
    // would fall through to the outliner and split the block instead.
    interactionDelay: 0,
    // Entries come ranked and already filtered, so the list is asked again
    // for every keystroke rather than filtered from the last answer.
    activateOnTyping: true,
  });
}

function complete(context: CompletionContext, source: PropertySource): CompletionResult | null {
  if (!source.active()) return null;
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);

  const value = withValue.exec(before);
  if (value) {
    const [, lead, key, gap, typed] = value as unknown as [string, string, string, string, string];
    const hits = source.values(key.trim(), typed, MAX_HITS);
    if (hits.length === 0) return null;
    return {
      from: line.from + lead.length + key.length + 2 + gap.length,
      options: hits.map(
        (hit): Completion => ({
          label: hit.value,
          detail: hit.uses > 1 ? `${hit.uses} uses` : undefined,
        }),
      ),
      filter: false,
    };
  }

  const start = startOfKey.exec(before);
  if (!start) return null;
  const [, lead, typed] = start as unknown as [string, string, string];
  if (typed.trim() === "" && !context.explicit) return null;
  const hits = source.keys(typed, MAX_HITS);
  if (hits.length === 0) return null;
  return {
    from: line.from + lead.length,
    options: hits.map(
      (hit): Completion => ({
        label: hit.key,
        apply: `${hit.key}:: `,
        detail: useCount(hit.uses),
      }),
    ),
    filter: false,
  };
}
