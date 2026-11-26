import { useEffect, type RefObject } from 'react';

/**
 * What a dropdown menu does with the keyboard and the pointer while it is
 * open: focus goes to `first` (or the first item), the arrow keys wrap round
 * the items, Escape closes it and returns to its button, Tab or a press
 * elsewhere closes it.
 */
export function useMenu({
  open,
  close,
  button,
  menu,
  items = '[role="menuitem"]:not(:disabled)',
  first,
}: Readonly<{
  open: boolean;
  close: () => void;
  button: RefObject<HTMLElement | null>;
  menu: RefObject<HTMLElement | null>;
  /** A selector for the items the arrow keys visit. */
  items?: string;
  /** A selector for the item to focus on opening, if one is better than the first. */
  first?: string;
}>) {
  useEffect(() => {
    if (!open) {
      return;
    }
    const all = () => [
      ...(menu.current?.querySelectorAll<HTMLElement>(items) ?? []),
    ];
    (
      (first ? menu.current?.querySelector<HTMLElement>(first) : null) ??
      all()[0]
    )?.focus();
    const onPointerDown = (e: PointerEvent) => {
      if (
        e.target instanceof Node &&
        !menu.current?.contains(e.target) &&
        !button.current?.contains(e.target)
      ) {
        close();
      }
    };
    const onKey = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'Escape': {
          e.preventDefault();
          close();
          button.current?.focus();
          break;
        }
        case 'Tab': {
          close();
          break;
        }
        case 'ArrowDown':
        case 'ArrowUp': {
          e.preventDefault();
          const found = all();
          if (found.length === 0) {
            return;
          }
          const at = found.indexOf(document.activeElement as HTMLElement);
          const step = e.key === 'ArrowDown' ? 1 : -1;
          found[(at + step + found.length) % found.length]?.focus();
          break;
        }
        // No default
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    const current = menu.current;
    current?.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      current?.removeEventListener('keydown', onKey);
    };
  }, [open, close, button, menu, items, first]);
}
