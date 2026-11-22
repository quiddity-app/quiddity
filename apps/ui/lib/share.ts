const isPlain = (x: unknown): x is Record<string, unknown> => {
  if (typeof x !== 'object' || x === null) {
    return false;
  }
  const proto: unknown = Object.getPrototypeOf(x);
  return proto === Object.prototype || proto === null;
};

/**
 * `next`, but with every part that is equal to the same part of `previous`
 * being that part itself. A read of the graph makes new objects every time,
 * and React can only tell that a block hasn't changed, and skip drawing it,
 * if it is the very object it drew before. Only arrays and plain objects are
 * compared: that is all the replica's answers are made of.
 */
export function share<T>(previous: unknown, next: T): T {
  if (Object.is(previous, next)) {
    return next;
  }
  if (Array.isArray(next) && Array.isArray(previous)) {
    const parts = next.map((x: unknown, i) => share(previous[i], x));
    return (
      parts.length === previous.length &&
      parts.every((x, i) => x === previous[i])
        ? previous
        : parts
    ) as T;
  }
  if (isPlain(next) && isPlain(previous)) {
    const keys = Object.keys(next);
    const parts: Record<string, unknown> = {};
    for (const key of keys) {
      parts[key] = share(previous[key], next[key]);
    }
    const same =
      keys.length === Object.keys(previous).length &&
      keys.every((key) => key in previous && parts[key] === previous[key]);
    return (same ? previous : parts) as T;
  }
  return next;
}
