import { Annotation } from "@codemirror/state";

/** Marks changes that came from the graph, which must not go back to it. */
export const fromGraph = Annotation.define<boolean>();
