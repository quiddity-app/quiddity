'use client';

import { useEffect, useId, useRef, useState, type ReactNode } from 'react';

import Link from 'next/link';
import { useRouter } from 'next/navigation';

import { Region } from './boundary';
import { Outline } from './entity-view';
import { useView } from './use-view';

import type { EntityId } from '@/lib/model';

import { linkHint } from '@/lib/link-hint';
import { useGraph, useRead } from '@/lib/use-graph';

/** How long the pointer rests on a link before its preview appears. */
const PREVIEW_DELAY_MS = 300;

/** Where the preview goes: below the link, or above it in the lower half. */
function placement(rect: DOMRect): React.CSSProperties {
  const width = 320;
  const left = Math.max(
    8,
    Math.min(rect.left, document.documentElement.clientWidth - width - 8),
  );
  return rect.bottom > window.innerHeight / 2
    ? { left, bottom: window.innerHeight - rect.top + 4 }
    : { left, top: rect.bottom + 4 };
}

/**
 * A tooltip with an entity's title and its properties block. Not interactive
 * (it is `inert` and takes no pointer events), so the links in the block are
 * only drawn.
 */
function Preview({
  id,
  anchor,
  tooltipId,
}: Readonly<{ id: EntityId; anchor: DOMRect; tooltipId: string }>) {
  const entity = useRead((g) => g.entity(id));
  const properties = entity.blocks.filter((b) => b.propertiesBlock);
  return (
    <div
      id={tooltipId}
      role="tooltip"
      data-link-preview
      inert
      style={placement(anchor)}
      className="pointer-events-none fixed z-(--z-overlay) max-h-64 w-80 overflow-hidden rounded-lg border border-border bg-surface p-3 text-sm shadow-overlay"
    >
      <div className="font-semibold">{entity.title}</div>
      <div className="mt-1">
        <Outline blocks={properties} links={entity.links} />
      </div>
    </div>
  );
}

/**
 * Click opens in the main pane; shift-click opens at the side. Hovering or
 * focusing it names the entity in the corner toast, and after a moment shows
 * its title and properties block if it has one.
 */
export function EntityLink({
  id,
  className,
  title,
  children,
}: Readonly<{
  id: EntityId;
  className?: string;
  title?: string;
  children: ReactNode;
}>) {
  const view = useView();
  const router = useRouter();
  const graph = useGraph();
  const tooltipId = useId();
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const named = useRef<string | null>(null);

  const hide = () => {
    clearTimeout(timer.current);
    setAnchor(null);
    if (named.current !== null && linkHint.get() === named.current) {
      linkHint.hide();
    }
    named.current = null;
  };
  // A link removed while the pointer is on it.
  useEffect(
    () => () => {
      clearTimeout(timer.current);
      if (named.current !== null && linkHint.get() === named.current) {
        linkHint.hide();
      }
    },
    [],
  );
  // Escape dismisses the preview, which keeps the link in reach (WCAG 1.4.13).
  useEffect(() => {
    if (anchor === null) {
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setAnchor(null);
      }
    };
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
    };
  }, [anchor]);

  const show = (e: React.SyntheticEvent<HTMLAnchorElement>) => {
    let entity;
    try {
      entity = graph.entity(id);
    } catch {
      // Deleted since it was drawn: nothing to name.
      return;
    }
    named.current = entity.title;
    linkHint.show(entity.title);
    if (entity.blocks.some((b) => b.propertiesBlock)) {
      const rect = e.currentTarget.getBoundingClientRect();
      clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        setAnchor(rect);
      }, PREVIEW_DELAY_MS);
    }
  };

  return (
    <>
      <Link
        href={view.toMain(id)}
        className={className}
        // The native tooltip would sit over ours.
        title={anchor === null ? title : undefined}
        aria-describedby={anchor === null ? undefined : tooltipId}
        onMouseEnter={show}
        onFocus={show}
        onMouseLeave={hide}
        onBlur={hide}
        onClick={(e) => {
          hide();
          if (e.shiftKey) {
            e.preventDefault();
            router.push(view.toSide(id));
          }
        }}
      >
        {children}
      </Link>
      {anchor !== null && (
        <Region pending={null} failed={() => null}>
          <Preview id={id} anchor={anchor} tooltipId={tooltipId} />
        </Region>
      )}
    </>
  );
}
