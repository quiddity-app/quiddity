import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
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
const graph = process.env.QUIDDITY_STATIC_GRAPH;
const basePath = process.env.QUIDDITY_BASE_PATH ?? "";

if (graph) {
  mkdirSync("public", { recursive: true });
  copyFileSync(graph, join("public", "graph.loro"));
}

const config: NextConfig = {
  output: "export",
  trailingSlash: true,
  images: { unoptimized: true },
  ...(basePath && { basePath }),
  env: {
    // Replaced at build time, so the static backend is left out of the app.
    NEXT_PUBLIC_QUIDDITY_STATIC_GRAPH: graph ? "1" : "",
    NEXT_PUBLIC_QUIDDITY_BASE_PATH: basePath,
  },
  // `next dev` otherwise writes its own block into AGENTS.md whenever it
  // notices an agent, which is a hand-written file.
  agentRules: false,
};

export default config;
