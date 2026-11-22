'use client';

import { useState } from 'react';

import { Region, Skeleton } from './boundary';
import { EntityLink } from './entity-link';
import { Outline } from './entity-view';
import { ErrorLine, linkClass, sectionHeading, subsectionTitle } from './ui';

import type { EntityId, Reference } from '@/lib/model';

import { useDeferredRead } from '@/lib/use-graph';

function Groups({ references }: Readonly<{ references: Reference[] }>) {
  return (
    <ul className="space-y-4">
      {references.map(({ source, blocks, links }) => (
        <li key={source.id}>
          <h3 className={subsectionTitle}>
            <EntityLink id={source.id} className={linkClass}>
              {source.title}
            </EntityLink>
          </h3>
          <Outline blocks={blocks} links={links} />
        </li>
      ))}
    </ul>
  );
}

const count = (references: Reference[]) =>
  references.reduce((n, r) => n + r.blocks.length, 0);

function Linked({ id }: Readonly<{ id: EntityId }>) {
  const references = useDeferredRead((g) => g.references(id), [id]);
  if (references.length === 0) return null;
  return (
    <section aria-label="Linked references">
      <h2 className={`${sectionHeading} mb-3`}>
        Linked references ({count(references)})
      </h2>
      <Groups references={references} />
    </section>
  );
}

/** On a property's page: who has it, with what, a row per entity. */
function Uses({ id }: Readonly<{ id: EntityId }>) {
  const uses = useDeferredRead((g) => g.uses(id), [id]);
  if (uses.length === 0) return null;
  return (
    <section aria-label="Uses">
      <h2 className={`${sectionHeading} mb-3`}>Uses ({uses.length})</h2>
      <ul className="space-y-1">
        {uses.map(({ entity, values }) => (
          <li key={entity.id}>
            <EntityLink id={entity.id} className={linkClass}>
              {entity.title}
            </EntityLink>
            {values.some((v) => v !== '') && (
              <span className="text-ink-secondary">
                {': '}
                {values.filter((v) => v !== '').join('; ')}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

function UnlinkedList({ id }: Readonly<{ id: EntityId }>) {
  const references = useDeferredRead((g) => g.unlinkedReferences(id), [id]);
  if (references.length === 0) {
    return <p className="text-sm text-ink-muted">None.</p>;
  }
  return <Groups references={references} />;
}

/** Fetched only once opened, since finding them means reading every block. */
function Unlinked({ id }: Readonly<{ id: EntityId }>) {
  const [shown, setShown] = useState(false);
  return (
    <details onToggle={(e) => setShown(e.currentTarget.open)}>
      <summary className={`${sectionHeading} cursor-pointer select-none`}>
        Unlinked references
      </summary>
      {shown && (
        <div className="mt-3">
          <Region
            pending={<Skeleton className="h-4 w-1/2" />}
            failed={(error) => (
              <ErrorLine className="text-sm">{error.message}</ErrorLine>
            )}
          >
            <UnlinkedList id={id} />
          </Region>
        </div>
      )}
    </details>
  );
}

/** At the foot of an entity. Each part loads, and fails, on its own. */
export function References({ id }: Readonly<{ id: EntityId }>) {
  return (
    <footer className="mt-10 space-y-6 border-t border-border pt-6">
      <Region
        key={`linked:${id}`}
        pending={<Skeleton className="h-4 w-1/3" />}
        failed={(error) => (
          <ErrorLine className="text-sm">{error.message}</ErrorLine>
        )}
      >
        <Uses id={id} />
        <Linked id={id} />
      </Region>
      <Unlinked key={`unlinked:${id}`} id={id} />
    </footer>
  );
}
