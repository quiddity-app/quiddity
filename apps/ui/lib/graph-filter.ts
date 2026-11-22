import type { Connections } from './model';

/** What the graph view leaves out unless asked. */
export interface GraphShow {
  /** The lighter links a property makes to the entity that defines its key. */
  propertyLinks: boolean;
  journals: boolean;
  /** Pages with no links in or out. */
  orphans: boolean;
}

export const HIDE_ALL: GraphShow = {
  propertyLinks: false,
  journals: false,
  orphans: false,
};

/**
 * What to draw of `data`. **An orphan is judged on what is left** once journals
 * and property links have been hidden: a page only a hidden journal linked to,
 * or only a property joined, has nothing drawn to it, so it goes too.
 */
export function filterConnections(
  data: Connections,
  show: GraphShow,
): Connections {
  const shown = new Set(
    data.nodes.filter((n) => show.journals || !n.journal).map((n) => n.id),
  );
  const edges = data.edges.filter(
    (e) =>
      (show.propertyLinks || !e.weak) &&
      shown.has(e.source) &&
      shown.has(e.target),
  );
  const linked = new Set<string>();
  for (const { source, target } of edges) {
    linked.add(source);
    linked.add(target);
  }
  const nodes = data.nodes.filter(
    (n) => shown.has(n.id) && (show.orphans || linked.has(n.id)),
  );
  return { nodes, edges };
}
