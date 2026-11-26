import { useSyncExternalStore } from 'react';

/**
 * Below this the page is only a browser bar's height short, not a keyboard's.
 * A phone's keyboard is several times it.
 */
const KEYBOARD_MIN = 100;

/**
 * How much of the bottom of the layout viewport an on-screen keyboard covers,
 * or 0 if there isn't one. Browsers either shrink only the visual viewport
 * (so `position: fixed` stays behind the keyboard) or the layout one too (so
 * there is nothing to add); this is right for both.
 */
export function keyboardInset(
  innerHeight: number,
  visual: Pick<VisualViewport, 'height' | 'offsetTop'>,
): number {
  const covered = Math.round(innerHeight - visual.height - visual.offsetTop);
  return covered >= KEYBOARD_MIN ? covered : 0;
}

const read = () => {
  const visual = globalThis.visualViewport;
  return visual ? keyboardInset(globalThis.innerHeight, visual) : 0;
};

const subscribe = (notify: () => void) => {
  const visual = globalThis.visualViewport;
  visual?.addEventListener('resize', notify);
  visual?.addEventListener('scroll', notify);
  globalThis.addEventListener('resize', notify);
  return () => {
    visual?.removeEventListener('resize', notify);
    visual?.removeEventListener('scroll', notify);
    globalThis.removeEventListener('resize', notify);
  };
};

/** The height of the on-screen keyboard, in CSS pixels; 0 when it isn't up. */
export const useKeyboardInset = () =>
  useSyncExternalStore(subscribe, read, () => 0);
