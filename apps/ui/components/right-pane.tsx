"use client";

import Link from "next/link";
import { useState } from "react";
import type { EntityId } from "@/lib/model";
import { Region, Skeleton } from "./boundary";
import { EntityBody, EntityTitle } from "./entity-view";
import { useView } from "./use-view";

const iconButton =
  "rounded px-1.5 py-0.5 text-sm text-ink-muted hover:bg-surface-hover hover:text-ink";

/**
 * The header is outside the region that suspends, so a card can always be
 * collapsed or closed, even while its content is loading or has failed.
 */
function Secondary({ id }: { id: EntityId }) {
  const view = useView();
  const [collapsed, setCollapsed] = useState(false);
  return (
    <article className="rounded-lg border border-border bg-surface">
      <header className="flex items-center gap-1 px-3 py-1.5">
        <h3 className="min-w-0 flex-1 truncate text-sm font-medium">
          <Region
            pending={<Skeleton className="h-4 w-24" />}
            failed={() => <span className="text-ink-muted">{id}</span>}
          >
            <Link href={view.toMainFromSide(id)} className="hover:underline">
              <EntityTitle id={id} />
            </Link>
          </Region>
        </h3>
        <button
          type="button"
          className={iconButton}
          aria-expanded={!collapsed}
          aria-label={collapsed ? "Expand" : "Collapse"}
          onClick={() => setCollapsed((c) => !c)}
        >
          {collapsed ? "▸" : "▾"}
        </button>
        <Link href={view.toClosed(id)} className={iconButton} aria-label="Close">
          ✕
        </Link>
      </header>
      {!collapsed && (
        <div className="border-t border-border px-3 py-2 text-sm">
          <Region
            pending={
              <div className="space-y-2" role="status" aria-label="Loading">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-2/3" />
              </div>
            }
            failed={(error) => (
              <p role="alert" className="text-danger">
                {error.message}
              </p>
            )}
          >
            <EntityBody id={id} />
          </Region>
        </div>
      )}
    </article>
  );
}

export function RightPane() {
  const { open, toAllClosed } = useView();
  if (open.length === 0) return null;
  return (
    <>
      <div className="flex items-center justify-between px-3 py-2">
        <h2 className="text-xs font-medium uppercase tracking-wide text-ink-muted">
          Open
        </h2>
        <Link href={toAllClosed()} className={iconButton}>
          Close all
        </Link>
      </div>
      <div className="space-y-3 overflow-y-auto px-3 pb-3">
        {open.map((id) => (
          <Secondary key={id} id={id} />
        ))}
      </div>
    </>
  );
}
