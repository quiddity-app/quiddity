"use client";

import type { EntityId } from "@/lib/model";
import { useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { EntityLink } from "./entity-link";
import { Outline } from "./entity-view";
import { useView } from "./use-view";
import { linkClass, pageTitle } from "./ui";

/** One entity the link might mean: its title, and its properties block, which is what tells it apart. */
function Candidate({ id }: { id: EntityId }) {
  const entity = useRead((g) => g.entity(id));
  const properties = entity.blocks.filter((b) => b.propertiesBlock);
  return (
    <li data-candidate className="py-3">
      <h2 className="text-lg font-semibold">
        <EntityLink id={entity.id} className={linkClass}>
          {entity.title}
        </EntityLink>
      </h2>
      {properties.length > 0 ? (
        <div className="mt-1">
          <Outline blocks={properties} links={entity.links} />
        </div>
      ) : (
        <p className="mt-1 text-ink-muted">No properties.</p>
      )}
    </li>
  );
}

function Matches({ link }: { link: string }) {
  const ids = useRead((g) => g.linkMatches(link));
  if (ids.length === 0) {
    return <p className="text-ink-muted">Nothing matches this link.</p>;
  }
  return (
    <ul className="divide-y divide-border">
      {ids.map((id) => (
        <Candidate key={id} id={id} />
      ))}
    </ul>
  );
}

/** Where an ambiguous link goes: the entities it matches, each with its properties block. */
export function Disambiguation() {
  const { link } = useView();
  if (!link) {
    return <p className="mx-auto max-w-measure-text px-6 py-8">No link chosen.</p>;
  }
  return (
    <div className="mx-auto max-w-measure-text px-6 py-8">
      <h1 className={pageTitle}>
        Which <span className="font-mono">[[{link}]]</span>?
      </h1>
      <p className="mb-4 text-ink-secondary">
        Several entities match this link. Choose one, or add a qualifier to the link to say which.
      </p>
      <Region
        key={link}
        pending={<Skeleton className="h-16 w-full" />}
        failed={(error) => (
          <p role="alert" className="text-danger">
            {error.message}
          </p>
        )}
      >
        <Matches link={link} />
      </Region>
    </div>
  );
}
