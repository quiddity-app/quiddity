import type { View } from "./nav";

/**
 * Where the main pane is: its page, and the entity if it shows one. The side
 * panel is deliberately not part of it, so opening, closing and reordering
 * secondaries is never a step in the history.
 */
export type Place = Pick<View, "path" | "main">;

export const samePlace = (a: Place, b: Place) => a.path === b.path && a.main === b.main;

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
    return this.at < this.places.length - 1 ? this.step(this.at + 1) : undefined;
  }

  private step(to: number): Place {
    this.left.push(this.places[this.at]!);
    this.at = to;
    this.pending = this.places[to]!;
    return this.pending;
  }
}
