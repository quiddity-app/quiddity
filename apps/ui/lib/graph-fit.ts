/**
 * Choosing the pan and zoom that show part of the graph: which entities make
 * the main body, and the transform that frames some points in a viewport.
 */
export interface Disc {
  x: number;
  y: number;
  r: number;
}

export interface Fit {
  x: number;
  y: number;
  k: number;
}

/**
 * The entities in the largest connected group, so that a scatter of loose
 * entities doesn't shrink the view to a speck. When nothing is linked at all
 * every entity counts, as there is no main body to prefer.
 */
export function largestGroup(
  ids: readonly string[],
  edges: readonly { source: string; target: string }[],
): Set<string> {
  const parent = new Map(ids.map((id) => [id, id]));
  const find = (id: string): string => {
    const up = parent.get(id) ?? id;
    if (up === id) return id;
    const root = find(up);
    parent.set(id, root);
    return root;
  };
  for (const { source, target } of edges) {
    if (parent.has(source) && parent.has(target)) parent.set(find(source), find(target));
  }
  const groups = new Map<string, string[]>();
  for (const id of ids) {
    const root = find(id);
    (groups.get(root) ?? groups.set(root, []).get(root)!).push(id);
  }
  let best: string[] = [];
  for (const group of groups.values()) if (group.length > best.length) best = group;
  return new Set(best.length <= 1 ? ids : best);
}

/**
 * The transform that centres `discs` in a viewport of this size, with `pad`
 * (a fraction of their extent) round the edge, as large as `maxScale` allows
 * and no smaller than `minScale`. Null when there is nothing to frame.
 */
export function fitTransform(
  discs: readonly Disc[],
  width: number,
  height: number,
  { pad, minScale, maxScale }: { pad: number; minScale: number; maxScale: number },
): Fit | null {
  if (discs.length === 0 || width <= 0 || height <= 0) return null;
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const { x, y, r } of discs) {
    minX = Math.min(minX, x - r);
    maxX = Math.max(maxX, x + r);
    minY = Math.min(minY, y - r);
    maxY = Math.max(maxY, y + r);
  }
  const k = Math.max(
    minScale,
    Math.min(maxScale, width / ((maxX - minX) * (1 + 2 * pad)), height / ((maxY - minY) * (1 + 2 * pad))),
  );
  return {
    k,
    x: width / 2 - ((minX + maxX) / 2) * k,
    y: height / 2 - ((minY + maxY) / 2) * k,
  };
}
