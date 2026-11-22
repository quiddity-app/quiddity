/**
 * A debounce that can't be starved. `call` waits for `wait` ms of quiet, as a
 * plain debounce does, but never for more than `max` ms from the first call
 * since `run` last ran, so something kept busy (typing, say) still has its
 * work done every `max` ms instead of once it stops.
 */
export interface Debounced {
  /** Asks for `run`, soon: `wait` ms after the last call, and at the latest
   * `max` ms after the first one that `run` hasn't answered. */
  call(): void;
  /** Whether `run` is wanted and hasn't been yet. */
  readonly pending: boolean;
  /** Forgets a call that `run` hasn't answered. */
  cancel(): void;
}

export function debounce(
  run: () => void,
  wait: number,
  max: number,
): Debounced {
  let timer: ReturnType<typeof setTimeout> | undefined;
  // When the first unanswered call was made.
  let first = 0;
  const fire = () => {
    timer = undefined;
    run();
  };
  return {
    call() {
      const now = Date.now();
      if (timer === undefined) {
        first = now;
      } else {
        clearTimeout(timer);
      }
      timer = setTimeout(fire, Math.max(0, Math.min(wait, first + max - now)));
    },
    get pending() {
      return timer !== undefined;
    },
    cancel() {
      clearTimeout(timer);
      timer = undefined;
    },
  };
}
