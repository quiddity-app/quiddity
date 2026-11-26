/**
 * Why the app couldn't open the graph on this device, and where it is. The
 * files are never deleted: whichever way the user goes on, the damaged ones
 * are kept beside the graph.
 */
export interface Problem {
  dir: string;
  message: string;
}

/** How to begin on a device that has no graph: a graph of its own, or none yet, to be given one by another device. */
export type Begin = 'new' | 'join';

/** How to go on: look again, start empty to sync from another device, or start a graph of its own. */
export type Recovery = 'retry' | 'blank' | 'fresh';

/**
 * The graphs on this device, for the picker in the corner. `switchable` is
 * false when the app was told which graph to open, and has only that one.
 */
export interface GraphList {
  current: string;
  all: string[];
  switchable: boolean;
}
