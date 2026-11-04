/**
 * How big things are in the graph view. Node radii are in the graph's own
 * space, so they grow and shrink with the zoom; everything else (text, line
 * widths) is held at a size on screen, in CSS pixels, whatever the zoom and
 * whatever the display's pixel density.
 */

/** The radius of a node, in graph space: a floor so a leaf can be seen and
 * grabbed, then logarithmic so a hub stands out without swamping its
 * neighbours. */
export const radius = (degree: number): number => Math.max(3, Math.log(degree + 1) * 3);

/** The height of a label, in CSS pixels on screen. */
export const LABEL_PX = 10;

/** A label is drawn once its node is this many CSS pixels across on screen.
 * At normal zoom that is every node; zoomed out, the hubs keep their names and
 * the leaves lose them. */
export const LABEL_DIAMETER = 5;

/** Whether a node of this radius has room for its name at this zoom. */
export const labelFits = (r: number, k: number): boolean => 2 * r * k >= LABEL_DIAMETER;
