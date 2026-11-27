'use client';

import { useCallback, useEffect, useId, useRef, useState } from 'react';

import Link from 'next/link';
import { useRouter } from 'next/navigation';

import { noteFollowed } from './home';
import { useMenu } from './use-menu';
import { useView } from './use-view';
import { Button, iconButtonClass } from './ui';

import type { EntityId } from '@/lib/model';

import { listenToMenu, setEntityOpen } from '@/lib/menu';
import { entrypoints } from '@/lib/nav';
import { checkForUpdateNow, useUpdates } from '@/lib/updates';
import { useGraph, useRead } from '@/lib/use-graph';

const menuEntrypoints = entrypoints.filter((e) => e.in === 'menu');

const itemClass =
  'block w-full truncate rounded px-3 py-1.5 text-left text-sm hover:bg-surface-hover focus:bg-surface-hover disabled:text-ink-muted disabled:hover:bg-transparent coarse:py-3';

/**
 * The three-dot menu to the right of search: the entrypoints that aren't in
 * the sidebar, and what can be done to the entity in the main pane. Deleting
 * asks first, and is also what Delete Entity in the app's File menu does, so
 * the native menu's commands are heard here too.
 */
export function MoreMenu() {
  const router = useRouter();
  const graph = useGraph();
  const view = useView();
  const { supported } = useUpdates();
  const [open, setOpen] = useState(false);
  const [asking, setAsking] = useState<EntityId | null>(null);
  // An entity that has been confirmed, and is deleted once the page has left it.
  const [leaving, setLeaving] = useState<EntityId | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const id = useId();

  const latest = useRef(view);
  latest.current = view;
  const { main } = view;

  // The native menu: Go navigates as the sidebar does, which drops the side
  // pane, and Delete Entity asks about what the main pane shows.
  useEffect(
    () =>
      listenToMenu((command) => {
        if (command.kind === 'go') {
          noteFollowed();
          router.push(command.path);
        } else if (command.kind === 'check-for-updates') {
          void checkForUpdateNow();
        } else if (latest.current.main) {
          setAsking(latest.current.main);
        }
      }),
    [router],
  );
  useEffect(() => {
    setEntityOpen(main !== null);
  }, [main]);

  // The entity is deleted only after the page has moved off it: until then
  // the main pane would try to draw an entity that is gone.
  useEffect(() => {
    if (leaving === null || main === leaving) {
      return;
    }
    graph.deleteEntities([leaving]);
    setLeaving(null);
  }, [graph, leaving, main]);

  const close = useCallback(() => {
    setOpen(false);
  }, []);
  useMenu({ open, close, button, menu });

  const confirm = (entity: EntityId) => {
    setAsking(null);
    setLeaving(entity);
    // Back would land on an entity that has gone, so this replaces the step.
    router.replace(
      view.href({
        path: '/',
        main: null,
        open: view.open.filter((o) => o !== entity),
      }),
    );
  };

  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        aria-label="More"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? `${id}-menu` : undefined}
        className={iconButtonClass}
        onClick={() => {
          setOpen((o) => !o);
        }}
      >
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          className="h-5 w-5"
          fill="currentColor"
        >
          <circle cx="12" cy="5" r="1.8" />
          <circle cx="12" cy="12" r="1.8" />
          <circle cx="12" cy="19" r="1.8" />
        </svg>
      </button>
      {open && (
        <div
          ref={menu}
          id={`${id}-menu`}
          role="menu"
          aria-label="More"
          className="absolute right-0 top-full z-(--z-overlay) mt-1 min-w-48 rounded-lg border border-border bg-surface p-1 shadow-overlay"
        >
          {menuEntrypoints.map(({ title, href }) => (
            <Link
              key={href}
              href={href}
              role="menuitem"
              className={itemClass}
              onClick={() => {
                setOpen(false);
              }}
            >
              {title}
            </Link>
          ))}
          <button
            type="button"
            role="menuitem"
            disabled={main === null}
            className={itemClass}
            onClick={() => {
              setOpen(false);
              router.push(view.togglePageGraph());
            }}
          >
            {view.pageGraph ? 'Hide Page Graph' : 'Show Page Graph'}
          </button>
          {supported && (
            <button
              type="button"
              role="menuitem"
              className={itemClass}
              onClick={() => {
                setOpen(false);
                void checkForUpdateNow();
              }}
            >
              Check for Updates
            </button>
          )}
          <button
            type="button"
            role="menuitem"
            disabled={main === null}
            className={`${itemClass} text-danger`}
            onClick={() => {
              setOpen(false);
              if (main) {
                setAsking(main);
              }
            }}
          >
            Delete Entity
          </button>
        </div>
      )}
      {asking !== null && (
        <ConfirmDelete
          entity={asking}
          onConfirm={() => {
            confirm(asking);
          }}
          onCancel={() => {
            setAsking(null);
          }}
        />
      )}
    </div>
  );
}

/**
 * Asks before an entity is deleted, inline rather than with `window.confirm`,
 * which the app's webview can't be relied on to show. Cancel has the focus, so
 * a stray Enter keeps the entity.
 */
function ConfirmDelete({
  entity,
  onConfirm,
  onCancel,
}: Readonly<{
  entity: EntityId;
  onConfirm: () => void;
  onCancel: () => void;
}>) {
  const { title } = useRead((g) => g.entity(entity));
  const heading = useId();
  return (
    <div
      className="fixed inset-0 z-(--z-overlay) flex items-start justify-center p-4 pt-[12vh]"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          onCancel();
        }
      }}
    >
      <button
        type="button"
        tabIndex={-1}
        aria-label="Cancel deletion"
        className="absolute inset-0 bg-scrim"
        onClick={onCancel}
      />
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={heading}
        className="relative w-full max-w-md rounded-lg border border-border bg-surface p-4 shadow-overlay"
      >
        <p id={heading} className="mb-4">
          Delete “{title}” and everything in it? Links to it will point nowhere.
          Undo brings it back.
        </p>
        <div className="flex justify-end gap-2">
          <Button autoFocus onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="danger" onClick={onConfirm}>
            Delete
          </Button>
        </div>
      </div>
    </div>
  );
}
