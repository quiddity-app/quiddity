import type { EntityId } from '../model';
import type { Graph } from '../graph';

/** What choosing a hit does. Data rather than a callback where it can be, so
 * a hit can be kept, ranked and logged. */
export type Target =
  /** Open the entity: in the main pane, or at the side. */
  | { kind: 'entity'; id: EntityId }
  /** Anything else, such as an action. */
  | { kind: 'run'; run: () => void };

export interface Hit {
  /** Unique within its facet. */
  key: string;
  label: string;
  /** Runs of `label` to highlight, as ascending `[start, end)` UTF-16 units. */
  ranges?: [number, number][];
  /** A line of context under the label. */
  detail?: string;
  /** Orders hits within a facet; only comparable with the same facet's hits. */
  score: number;
  target: Target;
}

/**
 * One way of searching, shown as one section of the panel. Facets are
 * independent: the panel runs each on its own and shows its answer as it
 * arrives, so a slow one (full text, embeddings) never holds up a fast one.
 * Sections come in the order the facets are given; a facet returns its hits
 * best first.
 *
 * Build new facets from existing ones with the combinators below.
 */
export interface Facet {
  /** Unique among the facets in a panel. */
  id: string;
  /** The section's heading. */
  title: string;
  /** `query` is trimmed and not empty. */
  search(query: string, graph: Graph): Hit[] | Promise<Hit[]>;
}

/** At most `n` hits. */
export function limited(n: number, facet: Facet): Facet {
  return {
    ...facet,
    search: async (q, g) => (await facet.search(q, g)).slice(0, n),
  };
}

/** Searches only for queries that pass `applies`, which may also rewrite them
 * (to drop a prefix that selects the facet, say); others find nothing. */
export function when(
  applies: (query: string) => string | null,
  facet: Facet,
): Facet {
  return {
    ...facet,
    search: (q, g) => {
      const rest = applies(q)?.trim();
      return rest ? facet.search(rest, g) : [];
    },
  };
}
