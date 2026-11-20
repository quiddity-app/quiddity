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
  /** The blocks selected as blocks, which is not selecting text in one. None
   * is being edited while any is selected. */
  selected: ReadonlySet<string>;
  /** Marks this outline's rows (`data-outline`), so a press or a key can be
   * told from one in another outline on the page, such as at the side. */
  scope: string;
  /** Says something to a screen reader, politely: what an edit did that
   * can't otherwise be seen or heard, such as a block that has moved. */
  announce(text: string): void;
  edit(id: string, caret: Caret): void;
  /** Stops editing `id`, unless something else has been started since. */
  stop(id: string): void;
  /** Adds an empty first block for properties and edits it. It is dropped
   * again if it is left empty. */
  addProperties(): void;
  /** Whether `id` is a block the offer of properties made, which is a
   * properties block from the moment it is made, though it is empty. */
  isOffered(id: string): boolean;
}

export const EditingContext = createContext<Editing | null>(null);

/** The editing state of the enclosing outline, if it is editable. */
export const useEditing = () => useContext(EditingContext);
