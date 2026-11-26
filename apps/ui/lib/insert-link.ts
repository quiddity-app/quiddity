import { EditorSelection } from '@codemirror/state';

import type { EditorView } from '@codemirror/view';

/**
 * Makes a `[[link]]`: an empty one with the caret inside, or one around the
 * selection with the caret after it. Typed as input, so the page list opens
 * for an empty one the way it does when the brackets are typed.
 */
export function insertLink(view: EditorView): boolean {
  const { from, to } = view.state.selection.main;
  view.dispatch({
    changes: [
      { from, insert: '[[' },
      { from: to, insert: ']]' },
    ],
    selection:
      from === to
        ? EditorSelection.cursor(from + 2)
        : EditorSelection.cursor(to + 4),
    userEvent: 'input.type',
    scrollIntoView: true,
  });
  view.focus();
  return true;
}
