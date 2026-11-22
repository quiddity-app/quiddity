'use client';

import { useContext } from 'react';

import { BlockEditor } from './block-editor';
import { EditableOutline } from './editing';
import { EditingContext } from './editing-context';
import { RenderedBlock, splitBlock } from './rendered-block';

import type { Block, EntityId, ResolvedLink } from '@/lib/model';

import { useRead } from '@/lib/use-graph';

/** One block: its text, rendered or being edited, then its children. */
function BlockRow({
  block,
  links,
}: Readonly<{ block: Block; links: Record<string, ResolvedLink> }>) {
  const editing = useContext(EditingContext);
  const current = editing?.current;
  const isEditing = current?.id === block.id;
  const selected = editing?.selected.has(block.id) ?? false;
  // A block of nothing but properties is drawn as text lines, which need no bullet.
  // The bullet's place stays, so the lines line up with the text around them.
  const { prose, lineProps } = splitBlock(block);
  // A file has its bullet like any block: it is not only properties.
  const propsOnly =
    !isEditing && !block.blob && prose.trim() === '' && lineProps.length > 0;
  return (
    <li
      className={`flex gap-2 ${block.propertiesBlock ? 'rounded bg-surface-properties px-2' : ''}`}
      data-properties-block={block.propertiesBlock ? '' : undefined}
      data-block-id={editing ? block.id : undefined}
      data-outline={editing?.scope}
    >
      <span
        aria-hidden
        data-bullet
        className={`mt-2.5 size-1.5 shrink-0 rounded-full bg-bullet ${propsOnly ? 'invisible' : ''}`}
      />
      <div className="min-w-0 flex-1 py-0.5">
        {isEditing ? (
          <BlockEditor
            id={block.id}
            caret={current.caret}
            properties={
              block.propertiesBlock || (editing?.isOffered(block.id) ?? false)
            }
          />
        ) : (
          <div
            data-block-text
            // The block's own text, not its children's, so a selected parent and
            // child aren't painted twice.
            data-selected={selected ? '' : undefined}
            className={`min-h-[1.5em] break-words [&>*+*]:mt-1 ${editing ? 'cursor-text' : ''} ${selected ? 'rounded bg-selection' : ''}`}
            tabIndex={editing ? 0 : undefined}
            onClick={(e) => {
              // Links and chips do their own thing, and so does selecting text.
              const target = e.target as HTMLElement;
              if (!editing || target.closest('a, button, input')) return;
              if (window.getSelection()?.toString()) return;
              editing.edit(block.id, 'end');
            }}
            onKeyDown={(e) => {
              if (
                editing &&
                e.target === e.currentTarget &&
                e.key === 'Enter'
              ) {
                e.preventDefault();
                editing.edit(block.id, 'end');
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
}: Readonly<{
  blocks: Block[];
  links: Record<string, ResolvedLink>;
}>) {
  return (
    <ul className="space-y-1">
      {blocks.map((block) => (
        <BlockRow key={block.id} block={block} links={links} />
      ))}
    </ul>
  );
}

/** An entity's blocks, editable in place. Throws if there is no such entity. */
export function EntityBody({ id }: Readonly<{ id: EntityId }>) {
  const entity = useRead((g) => g.entity(id));
  return (
    <EditableOutline
      entityId={id}
      blocks={entity.blocks}
      links={entity.links}
    />
  );
}

export function EntityTitle({ id }: Readonly<{ id: EntityId }>) {
  return <>{useRead((g) => g.entity(id)).title}</>;
}
