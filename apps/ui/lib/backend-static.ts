import type { Backend } from './graph';

/**
 * A published graph: the snapshot sits beside the pages. Edits live in the
 * page and are gone on reload, as with the in-memory graph.
 */
export const staticBackend: Backend = {
  async load() {
    const url = `${process.env.NEXT_PUBLIC_QUIDDITY_BASE_PATH}/graph.loro`;
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`Couldn't load the graph: ${response.status}`);
    }
    return new Uint8Array(await response.arrayBuffer());
  },
  async push() {},
};
