import type { Facet } from './facet';

/** How much of a block to show, and how much before the first match. */
const WIDTH = 120;
const LEAD = 30;

/** Blocks by what they say: every word must be there, the last may be unfinished. */
export const text: Facet = {
  id: 'text',
  title: 'Text',
  search: (query, graph) =>
    graph
      .searchText(query, 50)
      .map(({ entity, block, text, score, ranges }) => {
        // Show the first match, not the start of a long block. Whitespace
        // becomes a space one for one, so the ranges still fit.
        let from = Math.max(0, (ranges[0]?.[0] ?? 0) - LEAD);
        // Don't start in the middle of a surrogate pair.
        const unit = text.codePointAt(from) ?? 0;
        if (from > 0 && unit >= 0xdc00 && unit <= 0xdfff) from++;
        const shown = text.slice(from, from + WIDTH).replace(/\s/g, ' ');
        const lead = from > 0 ? '…' : '';
        return {
          key: block,
          label: lead + shown,
          ranges: ranges
            .filter(([s, e]) => s >= from && e <= from + WIDTH)
            .map(([s, e]): [number, number] => [
              s - from + lead.length,
              e - from + lead.length,
            ]),
          detail: entity.title,
          score,
          target: { kind: 'entity', id: entity.id },
        };
      }),
};
