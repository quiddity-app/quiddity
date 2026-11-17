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
 * Zoomed out, the hubs keep their names and the leaves lose them. A leaf is
 * 6 across at zoom 1, so this wants a node a little over that before it is
 * named: at 6 every leaf was labelled and the names piled on each other. */
export const LABEL_DIAMETER = 10;

/** Whether a node of this radius has room for its name at this zoom. */
export const labelFits = (r: number, k: number): boolean => 2 * r * k >= LABEL_DIAMETER;

/** How hard a link pulls, before its ends' degrees are taken into account:
 * a link a property makes to the entity defining it is a little looser than
 * one a person wrote. */
export const linkWeight = (weak: boolean): number => (weak ? 0.8 : 1);

/** A spring's strength: d3's default, 1 over the fewer links of its two
 * ends, scaled by the link's weight, so a link between two hubs is slack. The
 * floor of 1 stops an unlinked end from making it infinite. */
export const linkStrength = (weight: number, a: number, b: number): number =>
  weight / Math.max(Math.min(a, b), 1);
