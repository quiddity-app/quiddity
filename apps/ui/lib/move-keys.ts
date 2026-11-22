/** Whether this is an Apple platform, where Command is the modifier. */
export const isMac = () => /Mac|iPhone|iPad/.test(navigator.platform);

/**
 * The keys that move a block up and down among its siblings: Cmd-Shift-arrow
 * on a Mac and Alt-Shift-arrow elsewhere. Alt-arrow is a Mac's word movement
 * and the browser's back and forward, and Ctrl-Shift-arrow selects words on
 * the other platforms.
 */
export const moveKeyName = (arrow: 'ArrowUp' | 'ArrowDown') =>
  `${isMac() ? 'Meta' : 'Alt'}-Shift-${arrow}`;

/** -1 for a key that moves a block up, 1 for down, otherwise null. */
export function moveDirection(e: KeyboardEvent): -1 | 1 | null {
  const modifiers = isMac()
    ? e.metaKey && !e.altKey && !e.ctrlKey
    : e.altKey && !e.metaKey && !e.ctrlKey;
  if (!modifiers || !e.shiftKey) return null;
  if (e.key === 'ArrowUp') return -1;
  if (e.key === 'ArrowDown') return 1;
  return null;
}
