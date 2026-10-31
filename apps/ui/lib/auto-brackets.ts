import { EditorSelection, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

/** What may follow a `[` for it to be closed: the end, space, or a closer. */
const closable = /^(\s|[\])}]|$)/;

/** Unmatched `[` before `at` on its line. */
const open = (line: string, at: number) => {
  let depth = 0;
  for (let i = 0; i < at; i++) {
    if (line[i] === "\\") i++;
    else if (line[i] === "[") depth++;
    else if (line[i] === "]" && depth > 0) depth--;
  }
  return depth;
};

/**
 * Square brackets come in pairs: `[` makes `[]` with the caret between (so
 * `[[` makes `[[]]`), around a selection if there is one; `]` steps over a
 * closer it would duplicate; Backspace in an empty pair removes both.
 */
export function autoBrackets(): Extension {
  return [
    EditorView.inputHandler.of((view, from, to, text) => {
      if (view.composing) return false;
      const doc = view.state.doc;
      if (text === "[") {
        if (from !== to) {
          view.dispatch({
            changes: [
              { from, insert: "[" },
              { from: to, insert: "]" },
            ],
            selection: EditorSelection.range(from + 1, to + 1),
            userEvent: "input.type",
          });
          return true;
        }
        if (!closable.test(doc.sliceString(from, from + 1))) return false;
        view.dispatch({
          changes: { from, insert: "[]" },
          selection: EditorSelection.cursor(from + 1),
          userEvent: "input.type",
        });
        return true;
      }
      if (text === "]" && from === to && doc.sliceString(from, from + 1) === "]") {
        const line = doc.lineAt(from);
        if (open(line.text, from - line.from) > 0) {
          view.dispatch({ selection: EditorSelection.cursor(from + 1) });
          return true;
        }
      }
      return false;
    }),
    keymap.of([
      {
        key: "Backspace",
        run: (view) => {
          const { main } = view.state.selection;
          if (!main.empty) return false;
          const around = view.state.doc.sliceString(main.head - 1, main.head + 1);
          if (around !== "[]") return false;
          view.dispatch({
            changes: { from: main.head - 1, to: main.head + 1 },
            userEvent: "delete.backward",
          });
          return true;
        },
      },
    ]),
  ];
}
