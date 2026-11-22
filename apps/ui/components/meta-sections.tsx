'use client';

import { EntityLink } from './entity-link';
import { Value } from './property-chip';
import { linkClass, sectionHeading } from './ui';

import type { Entity, EntityId, InverseNode } from '@/lib/model';

import { useRead } from '@/lib/use-graph';

/** What doesn't fit a schema and has no chip of its own to show on. */
export function EntityFlags({ id }: Readonly<{ id: EntityId }>) {
  const { flags } = useRead((g) => g.entity(id));
  if (flags.length === 0) return null;
  return (
    <ul aria-label="Flags" className="mb-4 space-y-1 text-sm text-flag-text">
      {flags.map((flag, i) => (
        <li key={i}>{flag}</li>
      ))}
    </ul>
  );
}

/** Values the entity has from others, each saying where it is written. */
function InheritedValues({ entity }: Readonly<{ entity: Entity }>) {
  if (entity.inherited.length === 0) return null;
  return (
    <section aria-label="Inherited" className="mt-6">
      <h2 className={`${sectionHeading} mb-2`}>Inherited</h2>
      <ul className="flex flex-wrap gap-1.5">
        {entity.inherited.map((item, i) => (
          <li
            key={i}
            className="inline-flex items-baseline gap-1 rounded-full border border-dashed border-control px-2 py-0.5 text-xs leading-5"
          >
            <span className="text-ink-muted">
              {item.value === null ? `#${item.key}` : item.key}
            </span>
            {item.value !== null && (
              <Value value={item.value} links={entity.links} />
            )}
            <span className="text-ink-muted">
              from{' '}
              <EntityLink id={item.from.id} className={linkClass}>
                {item.from.title}
              </EntityLink>
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * Every row has a marker column of the same width, empty for a leaf, so a
 * title sits at its level's indent whether or not it can be expanded; the
 * browser's own marker would take room only on the rows that have one.
 */
const markerColumn = 'inline-block w-4 shrink-0';

function Tree({ nodes }: Readonly<{ nodes: InverseNode[] }>) {
  return (
    <ul className="space-y-0.5 pl-4">
      {nodes.map((node) => {
        const title = (
          <EntityLink
            id={node.entity.id}
            className={`${linkClass}${node.direct ? '' : ' italic'}`}
          >
            {node.entity.title}
          </EntityLink>
        );
        return (
          <li key={node.entity.id}>
            {node.children.length > 0 ? (
              <details open className="group">
                <summary className="flex cursor-pointer list-none items-baseline [&::-webkit-details-marker]:hidden">
                  <span
                    aria-hidden
                    className={`${markerColumn} text-ink-muted motion-safe:transition-transform group-open:rotate-90`}
                  >
                    ▸
                  </span>
                  {title}
                </summary>
                <Tree nodes={node.children} />
                {node.hidden > 0 && <More count={node.hidden} />}
              </details>
            ) : (
              <>
                <div className="flex items-baseline">
                  <span aria-hidden className={markerColumn} />
                  {title}
                </div>
                {node.hidden > 0 && <More count={node.hidden} />}
              </>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function More({ count }: Readonly<{ count: number }>) {
  return (
    <p className="pl-8 text-xs text-ink-muted">
      and {count} more, on its own page
    </p>
  );
}

/** For each inverse label, the entities related to this one the other way round. */
function Inverses({ entity }: Readonly<{ entity: Entity }>) {
  return (
    <>
      {entity.inverses.map(({ label, tree }) => (
        <section key={label} aria-label={label} className="mt-6">
          <h2 className={`${sectionHeading} mb-2`}>{label}</h2>
          <div className="-ml-4">
            <Tree nodes={tree} />
          </div>
        </section>
      ))}
    </>
  );
}

/** What meta-properties add below an entity's body. */
export function MetaSections({ id }: Readonly<{ id: EntityId }>) {
  const entity = useRead((g) => g.entity(id));
  return (
    <>
      <InheritedValues entity={entity} />
      <Inverses entity={entity} />
    </>
  );
}
