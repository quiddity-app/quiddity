import { parseView, type View } from './nav';

/**
 * Where the main pane is: its page, and the entity if it shows one. The side
 * panel is deliberately not part of it, so opening, closing and reordering
 * secondaries is never a step in the history.
 */
export type Place = Pick<View, 'path' | 'main'>;

/**
 * The place an address is, as far as can be told without the graph, or
 * `undefined` where that isn't a place yet: `/` and an entity page with no
 * entity only redirect, and a published page's name needs the graph to become
 * an entity. What `PlaceHistory.observe` is later given for it must be equal,
 * or the first place would be seen twice.
 */
export function placeAt(pathname: string, search: string): Place | undefined {
  if (pathname.startsWith('/page/')) return undefined;
  const { path, main } = parseView(pathname, new URLSearchParams(search));
  return path === '/' && !main ? undefined : { path, main };
}

/**
 * The place the page was opened at, in the browser. Things that remember places
 * mount only once the URL can be read, which can be after a link has been
 * followed, so each starts from here rather than from the first place it sees.
 */
export function placeOpened(): Place | undefined {
  if (typeof window === 'undefined') return undefined;
  const base = process.env.NEXT_PUBLIC_QUIDDITY_BASE_PATH ?? '';
  const { pathname, search } = window.location;
  const inApp =
    base && pathname.startsWith(base)
      ? pathname.slice(base.length) || '/'
      : pathname;
  return placeAt(inApp, search);
}

export const samePlace = (a: Place, b: Place) =>
  a.path === b.path && a.main === b.main;

/**
 * The main pane's own back/forward history. The browser's would also replay
 * the side panel, since it is in the URL, so the app keeps this instead and
 * traverses it with `router.replace`.
 *
 * The pane is only seen after it has rendered, which can be a moment after a
 * step was taken. A step made in that gap would otherwise be undone by the
 * late sighting of the place just left, which would look like a visit and
 * forget what was forward. So `observe` ignores places that were stepped away
 * from until the place stepped to has been seen.
 */
export class PlaceHistory {
  private places: Place[] = [];
  private at = -1;
  private left: Place[] = [];
  private pending: Place | undefined;

  get current(): Place | undefined {
    return this.places[this.at];
  }

  /** The main pane has gone somewhere new: forwards is forgotten, as in a browser. */
  visit(place: Place): void {
    if (this.current && samePlace(this.current, place)) return;
    this.places = [...this.places.slice(0, this.at + 1), place];
    this.at = this.places.length - 1;
  }

  /** The pane was seen showing this place, however it got there. */
  observe(place: Place): void {
    if (this.pending && samePlace(this.pending, place)) {
      this.pending = undefined;
      this.left = [];
      return;
    }
    if (this.left.some((l) => samePlace(l, place))) return;
    this.pending = undefined;
    this.left = [];
    this.visit(place);
  }

  /** Steps back and returns where to, or `undefined` at the start. */
  back(): Place | undefined {
    return this.at > 0 ? this.step(this.at - 1) : undefined;
  }

  /** Steps forward and returns where to, or `undefined` at the end. */
  forward(): Place | undefined {
    return this.at < this.places.length - 1
      ? this.step(this.at + 1)
      : undefined;
  }

  private step(to: number): Place {
    this.left.push(this.places[this.at]!);
    this.at = to;
    this.pending = this.places[to]!;
    return this.pending;
  }
}
