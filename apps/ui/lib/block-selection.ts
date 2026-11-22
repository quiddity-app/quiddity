/** The part of a block that selecting cares about. */
export interface Node {
  id: string;
  children: Node[];
}

function subtree(node: Node, out = new Set<string>()): Set<string> {
  out.add(node.id);
  for (const child of node.children) subtree(child, out);
  return out;
}

/** Each block from the top down to `id`, with the blocks it sits among. */
function pathTo(
  blocks: Node[],
  id: string,
): { node: Node; siblings: Node[] }[] | null {
  for (const node of blocks) {
    if (node.id === id) return [{ node, siblings: blocks }];
    const below = pathTo(node.children, id);
    if (below) return [{ node, siblings: blocks }, ...below];
  }
  return null;
}

/**
 * What a Select All from `anchor` selects next, or `null` when everything is
 * already selected.
 *
 * Each step takes in more of the outline around the block, working upwards:
 * the block; it and its descendants; it and its siblings, with their
 * descendants (its parent's children); then the parent with all of that, then
 * the parent's siblings, and so on, until the top level has everything. A step
 * that adds nothing (a block with no children, an only child) is skipped. The
 * step chosen is the first that holds what is selected already and more, so
 * a selection made by dragging grows from where it is.
 */
export function expandSelection(
  blocks: Node[],
  anchor: string,
  selected: ReadonlySet<string>,
): Set<string> | null {
  const path = pathTo(blocks, anchor);
  if (!path) return null;
  const steps: Set<string>[] = [new Set([anchor])];
  for (const { node, siblings } of [...path].reverse()) {
    steps.push(subtree(node));
    const all = new Set<string>();
    for (const sibling of siblings) subtree(sibling, all);
    steps.push(all);
  }
  return (
    steps.find(
      (step) =>
        step.size > selected.size && [...selected].every((id) => step.has(id)),
    ) ?? null
  );
}

/**
 * The blocks from `from` to `to` in reading order, whichever is first. A block
 * that has gone, in an undo or from another device, leaves nothing selected.
 */
export function selectRange(
  order: string[],
  from: string,
  to: string,
): Set<string> {
  const a = order.indexOf(from);
  const b = order.indexOf(to);
  if (a < 0 || b < 0) return new Set();
  return new Set(order.slice(Math.min(a, b), Math.max(a, b) + 1));
}

/**
 * The last of the selected blocks, in reading order, that isn't inside another
 * selected one: what a paste goes after, since a selected block's children are
 * part of it and a paste among them would be in the middle of the selection.
 */
export function lastSelected(
  blocks: Node[],
  selected: ReadonlySet<string>,
): string | null {
  let found: string | null = null;
  const walk = (nodes: Node[]) => {
    for (const node of nodes) {
      if (selected.has(node.id)) found = node.id;
      else walk(node.children);
    }
  };
  walk(blocks);
  return found;
}
