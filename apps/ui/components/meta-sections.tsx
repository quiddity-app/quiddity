"use client";

import type { Entity, EntityId, InverseNode } from "@/lib/model";
import { useRead } from "@/lib/use-graph";
import { EntityLink } from "./entity-link";
import { Value } from "./property-chip";

const heading =
  "text-xs font-medium uppercase tracking-wide text-slate-500 dark:text-slate-400";
const link = "text-sky-700 underline-offset-2 hover:underline dark:text-sky-300";

/** What doesn't fit a schema and has no chip of its own to show on. */
export function EntityFlags({ id }: { id: EntityId }) {
  const { flags } = useRead((g) => g.entity(id));
  if (flags.length === 0) return null;
  return (
    <ul aria-label="Flags" className="mb-4 space-y-1 text-sm text-amber-700 dark:text-amber-300">
      {flags.map((flag, i) => (
        <li key={i}>{flag}</li>
      ))}
    </ul>
  );
}

/** Values the entity has from others, each saying where it is written. */
function InheritedValues({ entity }: { entity: Entity }) {
  if (entity.inherited.length === 0) return null;
  return (
    <section aria-label="Inherited" className="mt-6">
      <h2 className={`${heading} mb-2`}>Inherited</h2>
      <ul className="flex flex-wrap gap-1.5">
        {entity.inherited.map((item, i) => (
          <li
            key={i}
            className="inline-flex items-baseline gap-1 rounded-full border border-dashed border-slate-300 px-2 py-0.5 text-xs leading-5 dark:border-slate-600"
          >
            <span className="text-slate-500 dark:text-slate-400">
              {item.value === null ? `#${item.key}` : item.key}
            </span>
            {item.value !== null && <Value value={item.value} links={entity.links} />}
            <span className="text-slate-400 dark:text-slate-500">
              from{" "}
              <EntityLink id={item.from.id} className={link}>
                {item.from.title}
              </EntityLink>
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Tree({ nodes }: { nodes: InverseNode[] }) {
  return (
    <ul className="space-y-0.5 pl-4">
      {nodes.map((node) => (
        <li key={node.entity.id}>
          {node.children.length > 0 ? (
            <details open>
              <summary className="cursor-pointer">
                <EntityLink id={node.entity.id} className={link}>
                  {node.entity.title}
                </EntityLink>
              </summary>
              <Tree nodes={node.children} />
              {node.hidden > 0 && <More count={node.hidden} />}
            </details>
          ) : (
            <>
              <EntityLink id={node.entity.id} className={link}>
                {node.entity.title}
              </EntityLink>
              {node.hidden > 0 && <More count={node.hidden} />}
            </>
          )}
        </li>
      ))}
    </ul>
  );
}

function More({ count }: { count: number }) {
  return (
    <p className="pl-4 text-xs text-slate-500 dark:text-slate-400">
      and {count} more, on its own page
    </p>
  );
}

/** For each inverse label, the entities related to this one the other way round. */
function Inverses({ entity }: { entity: Entity }) {
  return (
    <>
      {entity.inverses.map(({ label, tree }) => (
        <section key={label} aria-label={label} className="mt-6">
          <h2 className={`${heading} mb-2`}>{label}</h2>
          <div className="-ml-4">
            <Tree nodes={tree} />
          </div>
        </section>
      ))}
    </>
  );
}

/** What meta-properties add below an entity's body. */
export function MetaSections({ id }: { id: EntityId }) {
  const entity = useRead((g) => g.entity(id));
  return (
    <>
      <InheritedValues entity={entity} />
      <Inverses entity={entity} />
    </>
  );
}
