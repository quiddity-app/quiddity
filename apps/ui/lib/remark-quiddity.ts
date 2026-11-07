import type { PhrasingContent, Root } from "mdast";
import { visit } from "unist-util-visit";

/** Stands in for an inline property in the text the Markdown parser sees. */
export const PROP_OPEN = "";
export const PROP_CLOSE = "";

// `[label](dest)` where dest is an entity link or block reference. Markdown
// reads these itself unless the name has a space, so the text node is searched
// for them too; the label here is plain text, as the Markdown parser has
// already taken the formatted ones.
const TOKEN = new RegExp(
  `\\[([^\\[\\]]+)\\]\\(\\s*(?:\\[\\[([^\\]]+)\\]\\]|\\(\\(([^)\\s]+)\\)\\))\\s*\\)|` +
    `\\[\\[([^\\]]+)\\]\\]|\\(\\(([^)]+)\\)\\)|${PROP_OPEN}(\\d+)${PROP_CLOSE}`,
  "g",
);

const custom = (
  name: string,
  properties: Record<string, string>,
  text: string,
): PhrasingContent =>
  ({
    type: "text",
    value: text,
    data: { hName: name, hProperties: properties },
  }) as PhrasingContent;

export function remarkQuiddity() {
  return (tree: Root) => {
    // Markdown links whose destination is an entity or a block, such as
    // `[text]([[Name]])`, keep their formatted children.
    visit(tree, "link", (node) => {
      const entity = /^\[\[(.+)\]\]$/.exec(node.url);
      const block = /^\(\(([^\s()]+)\)\)$/.exec(node.url);
      if (!entity && !block) return;
      node.data = {
        hName: "quiddity-to",
        hProperties: entity ? { kind: "entity", to: entity[1]! } : { kind: "block", to: block![1]! },
      };
    });
    visit(tree, "text", (node, index, parent) => {
      if (!parent || index === undefined) return;
      const parts: PhrasingContent[] = [];
      let last = 0;
      for (const m of node.value.matchAll(TOKEN)) {
        if (m.index > last) parts.push({ type: "text", value: node.value.slice(last, m.index) });
        if (m[1] !== undefined) {
          const props =
            m[2] !== undefined ? { kind: "entity", to: m[2] } : { kind: "block", to: m[3]! };
          parts.push(custom("quiddity-to", props, m[1]));
        } else if (m[4] !== undefined) parts.push(custom("quiddity-link", { name: m[4] }, m[4]));
        else if (m[5] !== undefined) parts.push(custom("quiddity-ref", { id: m[5] }, m[0]));
        else parts.push(custom("quiddity-prop", { index: m[6]! }, ""));
        last = m.index + m[0].length;
      }
      if (parts.length === 0) return;
      if (last < node.value.length) parts.push({ type: "text", value: node.value.slice(last) });
      parent.children.splice(index, 1, ...parts);
      return index + parts.length;
    });
  };
}
