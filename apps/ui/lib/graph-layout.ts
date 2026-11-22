import type { EntityId } from './model';

/**
 * Where the graph view put each entity, and how it was panned and zoomed, so
 * that leaving the view and coming back finds everything where it was. Kept
 * in memory for navigation and in localStorage for the next start. Positions
 * are for the entities there were when last remembered: those since gone are
 * dropped, and an entity with no position is placed afresh.
 */
export interface Point {
  x: number;
  y: number;
}

export interface Transform extends Point {
  k: number;
}

const KEY = 'quiddity:graph-layout';
let positions = new Map<EntityId, Point>();
let transform: Transform | null = null;
let loaded = false;

const isNumber = (x: unknown): x is number =>
  typeof x === 'number' && Number.isFinite(x);

function load() {
  if (loaded) return;
  loaded = true;
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (typeof stored !== 'object' || stored === null) return;
    const { positions: saved, transform: view } = stored as Record<
      string,
      unknown
    >;
    if (typeof saved === 'object' && saved !== null) {
      for (const [id, p] of Object.entries(saved)) {
        const { x, y } = (p ?? {}) as Partial<Point>;
        if (isNumber(x) && isNumber(y)) positions.set(id, { x, y });
      }
    }
    const { x, y, k } = (view ?? {}) as Partial<Transform>;
    if (isNumber(x) && isNumber(y) && isNumber(k) && k > 0)
      transform = { x, y, k };
  } catch {
    // Unreadable or no storage: start from nothing.
  }
}

export function cachedPosition(id: EntityId): Point | undefined {
  load();
  return positions.get(id);
}

export function cachedTransform(): Transform | null {
  load();
  return transform;
}

/** Remembers where these entities are, and forgets any others. */
export function remember(
  nodes: Iterable<{ id: EntityId; x?: number; y?: number }>,
  view: Transform | null,
) {
  load();
  const next = new Map<EntityId, Point>();
  for (const { id, x, y } of nodes) {
    if (isNumber(x) && isNumber(y)) next.set(id, { x, y });
  }
  positions = next;
  transform = view ?? transform;
  try {
    localStorage.setItem(
      KEY,
      JSON.stringify({ positions: Object.fromEntries(next), transform }),
    );
  } catch {
    // Private mode or quota: the layout just won't outlive this session.
  }
}
