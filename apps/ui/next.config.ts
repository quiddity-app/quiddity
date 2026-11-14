import { copyFileSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import type { NextConfig } from "next";

// The UI ships inside Tauri as static files, so there is no server: every
// page is prerendered, and entities are addressed by search params rather than
// dynamic path segments (which would need every id known at build time).
//
// Setting QUIDDITY_STATIC_GRAPH to a snapshot file (as written by
// `quiddity-node snapshot`, never the graph's own file, which holds the
// device's key) instead builds a site showing that graph, for hosting
// somewhere like GitHub Pages: the snapshot is published beside the pages
// and loaded by the browser. QUIDDITY_BASE_PATH is where the site is
// served from, such as `/quiddity`. Without the variable the build is the
// one the Tauri shell embeds, and nothing here changes it.
//
// A published site also addresses its pages by name, which a rebuild leaves
// alone where an ID would not: `quiddity-node names` writes the names, by
// default beside the snapshot as `names.txt` (QUIDDITY_STATIC_NAMES says where
// else), and refuses a graph in which two entities share one.
const graph = process.env.QUIDDITY_STATIC_GRAPH;
const basePath = process.env.QUIDDITY_BASE_PATH ?? "";

let names: string[] = [];
if (graph) {
  mkdirSync("public", { recursive: true });
  copyFileSync(graph, join("public", "graph.loro"));
  const namesFile = resolve(
    process.env.QUIDDITY_STATIC_NAMES ?? join(dirname(graph), "names.txt"),
  );
  // Next reads this in the processes that prerender, which have the variable
  // only if it is set here.
  process.env.QUIDDITY_STATIC_NAMES = namesFile;
  names = readFileSync(namesFile, "utf8").split("\n").filter(Boolean);
}

const config: NextConfig = {
  output: "export",
  trailingSlash: true,
  images: { unoptimized: true },
  // `app/page/[name]/page.published.tsx` is a route only when there are names
  // to build it for: an export refuses a dynamic route with none.
  pageExtensions: ["tsx", "ts", "jsx", "js", ...(graph ? ["published.tsx"] : [])],
  ...(basePath && { basePath }),
  env: {
    // Replaced at build time, so the static backend is left out of the app.
    NEXT_PUBLIC_QUIDDITY_STATIC_GRAPH: graph ? "1" : "",
    NEXT_PUBLIC_QUIDDITY_BASE_PATH: basePath,
    // Which entities have a page by name; empty, so nothing is, in the app.
    NEXT_PUBLIC_QUIDDITY_PAGE_NAMES: graph ? JSON.stringify(names) : "",
  },
  // `next dev` otherwise writes its own block into AGENTS.md whenever it
  // notices an agent, which is a hand-written file.
  agentRules: false,
};

export default config;
