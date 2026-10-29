"use client";

import { useContext } from "react";
import type { Block, EntityId, ResolvedLink } from "@/lib/model";
import { useRead } from "@/lib/use-graph";
import { BlockEditor } from "./block-editor";
import { EditableOutline } from "./editing";
import { EditingContext } from "./editing-context";
import { RenderedBlock, splitBlock } from "./rendered-block";

/** One block: its text, rendered or being edited, then its children. */
function BlockRow({ block, links }: { block: Block; links: Record<string, ResolvedLink> }) {
  const editing = useContext(EditingContext);
  const current = editing?.current;
  const isEditing = current?.id === block.id;
  // A block of nothing but properties is drawn as chips, which need no bullet.
  // The bullet's place stays, so the chips line up with the text around them.
  const { prose, lineProps } = splitBlock(block);
  const chipsOnly = !isEditing && prose.trim() === "" && lineProps.length > 0;
  return (
    <li className="flex gap-2">
      <span
        aria-hidden
        data-bullet
        className={`mt-2.5 size-1.5 shrink-0 rounded-full bg-slate-400 ${chipsOnly ? "invisible" : ""}`}
      />
      <div className="min-w-0 flex-1 py-0.5">
        {isEditing ? (
          <BlockEditor id={block.id} caret={current.caret} />
        ) : (
          <div
            data-block-text
            className={`min-h-[1.5em] break-words [&>*+*]:mt-1 ${editing ? "cursor-text" : ""}`}
            tabIndex={editing ? 0 : undefined}
            onClick={(e) => {
              // Links and chips do their own thing, and so does selecting text.
              const target = e.target as HTMLElement;
              if (!editing || target.closest("a, button, input")) return;
              if (window.getSelection()?.toString()) return;
              editing.edit(block.id, "end");
            }}
            onKeyDown={(e) => {
              if (editing && e.target === e.currentTarget && e.key === "Enter") {
                e.preventDefault();
                editing.edit(block.id, "end");
              }
            }}
          >
            <RenderedBlock block={block} links={links} />
          </div>
        )}
        {block.children.length > 0 && (
          <div className="mt-1">
            <Outline blocks={block.children} links={links} />
          </div>
        )}
      </div>
    </li>
  );
}

/** Blocks as an outline. Editable if inside an `EditableOutline`. */
export function Outline({
  blocks,
  links,
}: {
  blocks: Block[];
  links: Record<string, ResolvedLink>;
}) {
  return (
    <ul className="space-y-1">
      {blocks.map((block) => (
        <BlockRow key={block.id} block={block} links={links} />
      ))}
    </ul>
  );
}

/** An entity's blocks, editable in place. Throws if there is no such entity. */
export function EntityBody({ id }: { id: EntityId }) {
  const entity = useRead((g) => g.entity(id));
  return <EditableOutline entityId={id} blocks={entity.blocks} links={entity.links} />;
}

export function EntityTitle({ id }: { id: EntityId }) {
  return <>{useRead((g) => g.entity(id)).title}</>;
}
