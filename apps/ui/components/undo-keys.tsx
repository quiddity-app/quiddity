'use client';

import { useEffect } from 'react';

import { useGraph } from '@/lib/use-graph';

/**
 * Undo and redo when nothing is being typed in: the block editors and text
 * fields handle the keys themselves, and the browser does the rest of the
 * time. This is what lets the keys undo a block's deletion once its editor
 * has gone.
 */
export function UndoKeys() {
  const graph = useGraph();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) {
        return;
      }
      if (
        (e.target as HTMLElement).closest('input, textarea, [contenteditable]')
      ) {
        return;
      }
      const key = e.key.toLowerCase();
      const redo = (key === 'z' && e.shiftKey) || key === 'y';
      if (key !== 'z' && key !== 'y') {
        return;
      }
      e.preventDefault();
      if (redo) {
        graph.redo();
      } else {
        graph.undo();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [graph]);
  return null;
}
