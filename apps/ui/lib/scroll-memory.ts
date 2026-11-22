import { samePlace, type Place } from './history';

/**
 * Where the main pane was scrolled to on each place it has shown, so that going
 * back to one (a link followed and then undone, or Alt-Left) finds it as it was
 * left. Only the main pane's place counts, as in `PlaceHistory`: opening a card
 * at the side doesn't move the page.
 *
 * A place not seen before starts at the top, as a page opened afresh would.
 * It remembers the most recent places only, so that a long session doesn't grow
 * without bound.
 */
export class ScrollMemory {
  private tops = new Map<string, number>();
  private place: Place | undefined;
  private top = 0;

  constructor(private readonly capacity = 200) {}

  /** The pane's scroll position has changed to `top`, on the place it shows. */
  scrolled(top: number): void {
    this.top = top;
  }

  /**
   * The pane now shows `place`. Returns where to scroll it to: where it was
   * last left, or the top. Moving to the place already shown changes nothing
   * and returns the current position, so it is safe to call on every render.
   */
  arrive(place: Place): number {
    if (this.place && samePlace(this.place, place)) {
      return this.top;
    }
    if (this.place) {
      this.remember(this.place, this.top);
    }
    this.place = place;
    this.top = this.tops.get(key(place)) ?? 0;
    return this.top;
  }

  private remember(place: Place, top: number): void {
    const k = key(place);
    // Re-inserting moves it to the end, which is what the oldest-first eviction wants.
    this.tops.delete(k);
    this.tops.set(k, top);
    if (this.tops.size > this.capacity) {
      this.tops.delete(this.tops.keys().next().value!);
    }
  }
}

// `path` never contains a newline, so the pair can't be confused with another.
const key = (place: Place) => `${place.path}\n${place.main ?? ''}`;
