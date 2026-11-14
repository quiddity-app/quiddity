"use client";

import { createContext, useContext } from "react";
import type { Graph } from "@/lib/graph";
import type { EntityId } from "@/lib/model";

/** Where to put the caret when a block starts being edited. */
export type Caret = number | "start" | "end";

/** The state of editing one entity's outline. Absent in read-only outlines. */
export interface Editing {
  graph: Graph;
  entityId: EntityId;
  /** The block being edited, if any. */
  current: { id: string; caret: Caret } | null;
  /** Every block of the entity, in reading order. */
  order: string[];
  edit(id: string, caret: Caret): void;
  /** Stops editing `id`, unless something else has been started since. */
  stop(id: string): void;
  /** Adds an empty first block for properties and edits it. It is dropped
   * again if it is left empty. */
  addProperties(): void;
}

export const EditingContext = createContext<Editing | null>(null);

/** The editing state of the enclosing outline, if it is editable. */
export const useEditing = () => useContext(EditingContext);
