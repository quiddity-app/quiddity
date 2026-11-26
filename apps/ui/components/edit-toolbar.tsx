'use client';

import { useLayoutEffect, useRef } from 'react';
import { createPortal } from 'react-dom';

import { EditorView } from '@codemirror/view';

import { blockCommands } from './block-editor';
import { useEditing } from './editing-context';
import { iconButtonClass } from './ui';

import { insertLink } from '@/lib/insert-link';
import { useKeyboardInset } from '@/lib/keyboard-inset';

/**
 * Keeps the editor's focus: a press that took it would blur the editor, which
 * leaves the block. Both events, since touch browsers differ in which of the
 * two they leave to the page.
 */
const keepFocus = {
  onPointerDown: (e: React.SyntheticEvent) => e.preventDefault(),
  onMouseDown: (e: React.SyntheticEvent) => e.preventDefault(),
};

/**
 * Actions that are hard to do with a touch keyboard, fixed to the bottom of the
 * viewport just above it while a block is being edited. It is drawn only when
 * an on-screen keyboard is up, which is when it is needed; a physical keyboard
 * has the keys.
 */
export function EditToolbar() {
  const editing = useEditing();
  const inset = useKeyboardInset();
  const bar = useRef<HTMLDivElement>(null);
  const shown = !!editing?.current && inset > 0;
  // The pane that scrolls pads itself by what covers its foot (the toolbar and
  // the keyboard), so the last block can be scrolled clear of it and the caret
  // is kept clear of it too: `main` uses it as padding and scroll padding.
  useLayoutEffect(() => {
    if (!shown) {
      return undefined;
    }
    const root = document.documentElement.style;
    root.setProperty(
      '--edit-toolbar-space',
      `${inset + (bar.current?.offsetHeight ?? 0)}px`,
    );
    return () => {
      root.removeProperty('--edit-toolbar-space');
    };
  }, [shown, inset]);
  if (!editing?.current || !shown) {
    return null;
  }
  const { id } = editing.current;
  const commands = blockCommands(id, () => editing);
  const view = () => {
    const dom = document.activeElement?.closest<HTMLElement>('.cm-editor');
    return dom ? EditorView.findFromDOM(dom) : null;
  };
  const act = (run: (v: EditorView) => boolean) => () => {
    const v = view();
    if (v) {
      run(v);
    }
  };
  const items: [label: string, glyph: string, run: () => void][] = [
    ['Outdent', '⇤', act(commands.outdent)],
    ['Indent', '⇥', act(commands.indent)],
    ['Move up', '↑', act(commands.moveUp)],
    ['Move down', '↓', act(commands.moveDown)],
    ['Insert link', '[[ ]]', act(insertLink)],
    ['Undo', '↶', () => editing.graph.undo()],
    ['Redo', '↷', () => editing.graph.redo()],
  ];
  return createPortal(
    <div
      ref={bar}
      role="toolbar"
      aria-label="Block actions"
      style={{ bottom: inset }}
      className="fixed inset-x-0 z-(--z-overlay) flex justify-around overflow-x-auto border-t border-border bg-surface px-1 shadow-overlay"
      {...keepFocus}
    >
      {items.map(([label, glyph, run]) => (
        <button
          key={label}
          type="button"
          aria-label={label}
          title={label}
          className={`${iconButtonClass} min-h-11 min-w-11 flex-1 text-base`}
          onClick={run}
        >
          {glyph}
        </button>
      ))}
    </div>,
    document.body,
  );
}
