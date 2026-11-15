/**
 * What a published build's export reads to render pages on the server: the
 * replica's WASM, and the snapshot `next.config.ts` has copied into `public/`
 * to be published. `bootGraph` reaches this only when there is no `window`.
 *
 * Node's modules are fetched at run time, not imported, because the bundler
 * would otherwise try to put `node:fs` in the browser's bundle, and refuses.
 */
function read(file: string): Buffer {
  const fs = process.getBuiltinModule("node:fs");
  const path = process.getBuiltinModule("node:path");
  return fs.readFileSync(path.join(process.cwd(), file));
}

export const wasmBytes = () => read("lib/wasm/quiddity_wasm_bg.wasm");

export const snapshotBytes = () => new Uint8Array(read("public/graph.loro"));
