import type { Facet } from "./facet";

/** Pages by title or alias: fuzzy, so `QD` finds `Quiddity Design` (see `quiddity-view`'s search). */
export const titles: Facet = {
  id: "titles",
  title: "Pages",
  search: (query, graph) =>
    graph.searchTitles(query, 50).map(({ entity, score, ranges, alias }) => ({
      key: entity.id,
      // The ranges are in the alias when it was what matched.
      label: alias ?? entity.title,
      detail: alias === null ? undefined : `Also called ${entity.title}`,
      ranges,
      score,
      target: { kind: "entity", id: entity.id },
    })),
};
