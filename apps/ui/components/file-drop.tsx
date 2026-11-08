"use client";

import { useState, type ClipboardEvent, type DragEvent } from "react";
import { bootGraph } from "@/lib/graph";
import type { EntityId } from "@/lib/model";

const hasFiles = (data: DataTransfer | null) => data !== null && data.types.includes("Files");

/**
 * Lets files be dropped or pasted onto a page: spread `handlers` on the
 * element that holds it. Each file becomes an entity of its own, linked at the
 * end of the page. `problem` is why the last one couldn't be added, if it
 * couldn't, for the page to show.
 *
 * The handlers are capture-phase and stop the event: a block's editor would
 * otherwise read a dropped file as text and put it in the block. Text, which
 * has no files, is left alone. They are on the element itself, not a wrapper,
 * because the outline is found as its direct child.
 *
 * The graph is fetched when a file arrives, not when this renders, so that
 * the page doesn't wait on it.
 */
export function useFileDrop(id: EntityId) {
  const [problem, setProblem] = useState<string>();

  async function add(files: File[]) {
    setProblem(undefined);
    const graph = await bootGraph();
    if (!graph.canKeepFiles()) {
      setProblem("This build can’t keep files.");
      return;
    }
    for (const file of files) {
      try {
        await graph.addFile(id, file);
      } catch (error) {
        setProblem(`Couldn’t add ${file.name || "the file"}: ${(error as Error).message}`);
        return;
      }
    }
  }

  const handlers = {
    onDragOver(e: DragEvent) {
      // Without this the browser would open the file in place of the page.
      if (hasFiles(e.dataTransfer)) e.preventDefault();
    },
    onDropCapture(e: DragEvent) {
      if (!hasFiles(e.dataTransfer)) return;
      e.preventDefault();
      e.stopPropagation();
      void add([...e.dataTransfer.files]);
    },
    onPasteCapture(e: ClipboardEvent) {
      const files = [...e.clipboardData.files];
      if (files.length === 0) return;
      e.preventDefault();
      e.stopPropagation();
      void add(files);
    },
  };
  return { handlers, problem };
}
