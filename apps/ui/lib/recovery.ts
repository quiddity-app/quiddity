/**
 * Why the app couldn't open the graph on this device, and where it is. The
 * files are never deleted: whichever way the user goes on, the damaged ones
 * are kept beside the graph.
 */
export type Problem = { dir: string; message: string };

/** How to go on: look again, start empty to sync from another device, or start a graph of its own. */
export type Recovery = "retry" | "blank" | "fresh";
