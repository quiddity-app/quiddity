import { useSyncExternalStore } from 'react';

import type { EntityRef } from './model';

const KEY = 'quiddity:recents';
const LIMIT = 10;
const listeners = new Set<() => void>();
const empty: EntityRef[] = [];
let snapshot: EntityRef[] | undefined;

function isRef(x: unknown): x is EntityRef {
  return (
    typeof x === 'object' &&
    x !== null &&
    typeof (x as EntityRef).id === 'string' &&
    typeof (x as EntityRef).title === 'string'
  );
}

function read(): EntityRef[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    return Array.isArray(parsed) ? parsed.filter((item) => isRef(item)) : empty;
  } catch {
    return empty;
  }
}

export function recordVisit(entity: EntityRef) {
  const here = { id: entity.id, title: entity.title };
  const next = [here, ...read().filter((x) => x.id !== here.id)].slice(
    0,
    LIMIT,
  );
  snapshot = next;
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // Private mode or quota: recents just won't persist.
  }
  for (const l of listeners) {
    l();
  }
}

export function useRecents(): EntityRef[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => (snapshot ??= read()),
    () => empty,
  );
}
