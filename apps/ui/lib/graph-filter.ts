import type { Connections } from "./model";

/** What the graph view leaves out unless asked. */
export interface GraphShow {
  /** The lighter links a property makes to the entity that defines its key. */
  propertyLinks: boolean;
  journals: boolean;
  /** Pages with no links in or out. */
  orphans: boolean;
}

export const HIDE_ALL: GraphShow = { propertyLinks: false, journals: false, orphans: false };

/**
 * What to draw of `data`. **An orphan is judged on the whole graph**, property
 * links included, before anything is hidden: otherwise hiding journals would
 * turn the pages only they link to into orphans, and hide those too.
 */
export function filterConnections(data: Connections, show: GraphShow): Connections {
  const linked = new Set<string>();
  for (const { source, target } of data.edges) {
    linked.add(source);
    linked.add(target);
  }
  const nodes = data.nodes.filter(
    (n) => (show.journals || !n.journal) && (show.orphans || linked.has(n.id)),
  );
  const kept = new Set(nodes.map((n) => n.id));
  const edges = data.edges.filter(
    (e) => (show.propertyLinks || !e.weak) && kept.has(e.source) && kept.has(e.target),
  );
  return { nodes, edges };
}
