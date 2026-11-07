"use client";

import { useEffect } from "react";
import type { EntityId } from "@/lib/model";
import { recordVisit } from "@/lib/recents";
import { useRead } from "@/lib/use-graph";
import { AssetFile } from "./asset-file";
import { Region, Skeleton } from "./boundary";
import { EditableTitle } from "./entity-title";
import { EntityBody } from "./entity-view";
import { useFileDrop } from "./file-drop";
import { EntityFlags, MetaSections } from "./meta-sections";
import { References } from "./references";

function Visit({ id }: { id: EntityId }) {
  const { id: canonical, title } = useRead((g) => g.entity(id));
  useEffect(() => recordVisit({ id: canonical, title }), [canonical, title]);
  return null;
}

export function MainEntity({ id }: { id: EntityId }) {
  const { handlers, problem } = useFileDrop(id);
  return (
    // `cqw` needs a container to measure, and an element can't be its own, so
    // the article's text size is set from the wrapper's width.
    <div className="@container">
      <article
        className="mx-auto max-w-[50rem] px-6 py-8 [font-size:calc(1.25rem_+_max(0.325cqw,0.325lvh))]"
        {...handlers}
      >
        {problem && (
          <p role="alert" className="mb-2 text-red-700 dark:text-red-300">
            {problem}
          </p>
        )}
        {/* Keyed so navigating resets the boundary rather than keeping an error. */}
        <Region
          key={id}
          pending={
            <div className="space-y-3" role="status" aria-label="Loading">
              <Skeleton className="h-8 w-1/3" />
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-2/3" />
            </div>
          }
          failed={(error) => (
            <div role="alert">
              <h1 className="text-2xl font-semibold">Couldn't open this</h1>
              <p className="mt-2 text-red-700 dark:text-red-300">
                {error.message}
              </p>
            </div>
          )}
        >
          <Visit id={id} />
          <h1 className="mb-4 text-[1.5em] font-semibold">
            <EditableTitle id={id} />
          </h1>
          <EntityFlags id={id} />
          <AssetFile id={id} />
          <EntityBody id={id} />
          <MetaSections id={id} />
        </Region>
        <References id={id} />
      </article>
    </div>
  );
}
