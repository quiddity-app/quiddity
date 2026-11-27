'use client';

import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
} from 'react';
import { createPortal } from 'react-dom';

import { useMenu } from './use-menu';
import { Button, ErrorLine, Input } from './ui';

import type { GraphList } from '@/lib/recovery';

import { chooseBackend, type Backend } from '@/lib/graph';
import { forgetPlace } from '@/lib/last-place';
import { useGraph } from '@/lib/use-graph';

const itemClass =
  'block w-full truncate rounded px-3 py-1.5 text-left text-sm font-normal hover:bg-surface-hover focus:bg-surface-hover coarse:py-3';

/** What a new graph starts as: its own, or empty to be filled by syncing. */
interface Making {
  sync: boolean;
}

/**
 * The name of the graph that is open, which opens a menu to choose another or
 * to make one: empty and its own, or empty to be filled from another device.
 * Where there is only one graph (a browser, a published build) it is just the
 * name, "Quiddity".
 *
 * Choosing starts the page over, as recovering does: the page holds a replica
 * of the old graph, so it is flushed first, then the shell swaps its graph and
 * the page reloads from the new one's snapshot.
 */
export function GraphSwitcher() {
  const graph = useGraph();
  const [backend, setBackend] = useState<Backend | null>(null);
  const [graphs, setGraphs] = useState<GraphList | null>(null);
  const [open, setOpen] = useState(false);
  const [making, setMaking] = useState<Making | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const id = useId();

  useEffect(() => {
    let current = true;
    void (async () => {
      try {
        const chosen = await chooseBackend();
        const found = (await chosen.graphs?.()) ?? null;
        if (current && found?.switchable) {
          setBackend(chosen);
          setGraphs(found);
        }
      } catch (error) {
        console.info("Couldn't list the graphs", error);
      }
    })();
    return () => {
      current = false;
    };
  }, []);

  const close = useCallback(() => {
    setOpen(false);
  }, []);
  const stopMaking = useCallback(() => {
    setMaking(null);
  }, []);
  useMenu({
    open,
    close,
    button,
    menu,
    items: '[role^="menuitem"]',
    first: '[aria-checked="true"]',
  });

  if (!backend || !graphs) {
    return <span className="px-2">Quiddity</span>;
  }

  /** Saves what has been typed, asks the shell, and starts the page over.
   * Returns the reason if the shell said no. */
  const change = async (ask: () => Promise<void> | undefined) => {
    try {
      await graph.flush();
      await ask();
    } catch (error) {
      return String(error instanceof Error ? error.message : error);
    }
    // What this page holds is the old graph, and its address may name a page
    // the new one lacks, nor is where it was a place to return to.
    forgetPlace();
    window.location.replace('/');
    return null;
  };

  const choose = (name: string) => {
    setOpen(false);
    if (name !== graphs.current) {
      void change(() => backend.switchGraph?.(name)).then((reason) => {
        if (reason) {
          setFailure(reason);
        }
      });
    }
  };

  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? `${id}-menu` : undefined}
        aria-label={`Graph: ${graphs.current}`}
        className="flex max-w-full items-center gap-1 rounded px-2 py-1 hover:bg-surface-hover coarse:min-h-11"
        onClick={() => {
          setFailure(null);
          setOpen((o) => !o);
        }}
      >
        <span className="truncate">{graphs.current}</span>
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          className="h-4 w-4 shrink-0"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      {failure && !making && (
        <ErrorLine className="px-2 text-sm">{failure}</ErrorLine>
      )}
      {open && (
        <div
          ref={menu}
          id={`${id}-menu`}
          role="menu"
          aria-label="Graphs"
          className="absolute left-0 top-full z-(--z-overlay) mt-1 max-h-[60vh] min-w-56 overflow-y-auto rounded-lg border border-border bg-surface p-1 shadow-overlay"
        >
          {graphs.all.map((name) => (
            <button
              key={name}
              type="button"
              role="menuitemradio"
              aria-checked={name === graphs.current}
              className={`${itemClass} ${name === graphs.current ? 'font-semibold' : ''}`}
              onClick={() => {
                choose(name);
              }}
            >
              {name}
            </button>
          ))}
          <div role="separator" className="my-1 border-t border-border" />
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={() => {
              setOpen(false);
              setMaking({ sync: false });
            }}
          >
            New empty graph…
          </button>
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={() => {
              setOpen(false);
              setMaking({ sync: true });
            }}
          >
            New graph to sync…
          </button>
        </div>
      )}
      {making && (
        <NewGraph
          sync={making.sync}
          onCancel={stopMaking}
          onCreate={(name) =>
            change(() => backend.createGraph?.(name, making.sync))
          }
        />
      )}
    </div>
  );
}

/**
 * Asks for the new graph's name, inline rather than with `window.prompt`,
 * which the app's webview can't be relied on to show. The shell's reason for
 * refusing a name (taken, or not one a directory can have) is shown here.
 */
function NewGraph({
  sync,
  onCreate,
  onCancel,
}: Readonly<{
  sync: boolean;
  /** Resolves to why it couldn't be made, or null once the page is leaving. */
  onCreate: (name: string) => Promise<string | null>;
  onCancel: () => void;
}>) {
  const [name, setName] = useState('');
  const [working, setWorking] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const heading = useId();
  const field = useId();

  // The dialog is modal, so Escape is heard wherever the focus is in it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
    };
  }, [onCancel]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setWorking(true);
    setFailure(null);
    void onCreate(name).then((reason) => {
      if (reason) {
        setFailure(reason);
        setWorking(false);
      }
    });
  };

  // In the body: the left pane is transformed, which would make `fixed` mean
  // that pane rather than the window.
  return createPortal(
    <div className="fixed inset-0 z-(--z-overlay) flex items-start justify-center p-4 pt-[12vh] font-normal">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Cancel"
        className="absolute inset-0 bg-scrim"
        onClick={onCancel}
      />
      <form
        role="dialog"
        aria-modal="true"
        aria-labelledby={heading}
        className="relative w-full max-w-md rounded-lg border border-border bg-surface p-4 text-base shadow-overlay"
        onSubmit={submit}
      >
        <h2 id={heading} className="mb-2 font-semibold">
          {sync ? 'New graph to sync' : 'New empty graph'}
        </h2>
        <p className="mb-3 text-sm text-ink-muted">
          {sync
            ? 'It starts with nothing, and fills when this device syncs with another that has the graph. Let that device in from Devices.'
            : 'It starts with nothing of yours in it, and is a graph of its own: it won’t merge with any other.'}
        </p>
        <label htmlFor={field} className="mb-1 block text-sm">
          Name
        </label>
        <Input
          id={field}
          autoFocus
          required
          maxLength={64}
          autoComplete="off"
          className="mb-3 w-full"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
          }}
        />
        {failure && <ErrorLine className="mb-3">{failure}</ErrorLine>}
        <div className="flex justify-end gap-2">
          <Button onClick={onCancel}>Cancel</Button>
          <Button type="submit" disabled={working || name.trim() === ''}>
            Create
          </Button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
