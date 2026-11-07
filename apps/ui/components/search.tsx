"use client";

import { useRouter } from "next/navigation";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { limited, type Facet, type Hit } from "@/lib/search/facet";
import { text } from "@/lib/search/text";
import { titles } from "@/lib/search/titles";
import { useSearch } from "@/lib/search/use-search";
import { useView } from "./use-view";

/** The sections of the panel, in order. New kinds of search are added here. */
const facets: Facet[] = [limited(8, titles), limited(8, text)];

const isMac = () => /Mac|iPhone|iPad/.test(navigator.platform);

/** The search button for the action strip, and the panel it opens (also on Cmd/Ctrl-K). */
export function Search() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    // Captured, so an editor that is focused can't take the key first.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  return (
    <>
      <button
        type="button"
        aria-haspopup="dialog"
        className="flex items-center gap-2 rounded border border-control px-2 py-1 text-sm text-ink-secondary hover:bg-surface-hover"
        onClick={() => setOpen(true)}
      >
        Search
        <kbd className="rounded bg-surface-hover px-1 text-xs">
          {isMac() ? "⌘K" : "Ctrl K"}
        </kbd>
      </button>
      {open && <Panel onClose={() => setOpen(false)} />}
    </>
  );
}

/** `label` with the runs in `ranges` marked. */
function Highlighted({ hit }: { hit: Hit }) {
  const parts: ReactNode[] = [];
  let at = 0;
  for (const [start, end] of hit.ranges ?? []) {
    if (start > at) parts.push(hit.label.slice(at, start));
    parts.push(
      <mark key={start} className="bg-transparent font-semibold text-link">
        {hit.label.slice(start, end)}
      </mark>,
    );
    at = end;
  }
  parts.push(hit.label.slice(at));
  return <>{parts}</>;
}

function Panel({ onClose }: { onClose: () => void }) {
  const router = useRouter();
  const view = useView();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const id = useId();

  const sections = useSearch(facets, query);
  const flat = sections.flatMap((s) => s.hits.map((hit) => ({ facet: s.facet, hit })));
  const current = Math.min(active, flat.length - 1);
  const optionId = (facet: Facet, hit: Hit) => `${id}-${facet.id}-${hit.key}`;

  // The panel takes the focus, and gives it back to where it came from.
  useEffect(() => {
    const before = document.activeElement;
    input.current?.focus();
    return () => {
      if (before instanceof HTMLElement) before.focus();
    };
  }, []);

  const choose = (hit: Hit, side: boolean) => {
    onClose();
    if (hit.target.kind === "run") hit.target.run();
    else router.push(side ? view.toSide(hit.target.id) : view.toMain(hit.target.id));
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = flat.length;
      if (n) setActive((Math.max(current, 0) + (e.key === "ArrowDown" ? 1 : n - 1)) % n);
    } else if (e.key === "Enter" && flat[current]) {
      e.preventDefault();
      choose(flat[current].hit, e.shiftKey);
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex items-start justify-center p-4 pt-[12vh]">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close search"
        className="absolute inset-0 bg-scrim"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search"
        className="relative flex max-h-[70vh] w-full max-w-xl flex-col overflow-hidden rounded-lg border border-border bg-surface shadow-xl"
        onKeyDown={onKeyDown}
      >
        <input
          ref={input}
          role="combobox"
          aria-expanded
          aria-controls={`${id}-results`}
          aria-activedescendant={
            flat[current] ? optionId(flat[current].facet, flat[current].hit) : undefined
          }
          aria-label="Search"
          placeholder="Search"
          autoComplete="off"
          spellCheck={false}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          className="w-full border-b border-border bg-transparent px-4 py-3 outline-none"
        />
        <div id={`${id}-results`} role="listbox" aria-label="Results" className="overflow-y-auto">
          {query.trim() && flat.length === 0 && (
            <p className="px-4 py-3 text-sm text-ink-muted">Nothing found.</p>
          )}
          {sections.map(({ facet, hits }) =>
            hits.length === 0 ? null : (
              <section key={facet.id} role="group" aria-label={facet.title} className="px-2 py-2">
                <h2 className="px-2 pb-1 text-xs font-medium uppercase tracking-wide text-ink-muted">
                  {facet.title}
                </h2>
                {hits.map((hit) => {
                  const selected = flat[current]?.hit === hit;
                  return (
                    <div
                      key={hit.key}
                      id={optionId(facet, hit)}
                      role="option"
                      aria-selected={selected}
                      // Keeps the focus in the field.
                      onMouseDown={(e) => e.preventDefault()}
                      onMouseMove={() => setActive(flat.findIndex((f) => f.hit === hit))}
                      onClick={(e) => choose(hit, e.shiftKey)}
                      className={`cursor-pointer truncate rounded px-2 py-1 text-sm ${
                        selected ? "bg-surface-hover" : ""
                      }`}
                    >
                      <Highlighted hit={hit} />
                      {hit.detail && (
                        <div className="truncate text-xs text-ink-muted">
                          {hit.detail}
                        </div>
                      )}
                    </div>
                  );
                })}
              </section>
            ),
          )}
        </div>
        <p className="border-t border-border px-4 py-1.5 text-xs text-ink-muted">
          ↑↓ to move · Enter to open · Shift-Enter to open at the side · Esc to close
        </p>
      </div>
    </div>
  );
}
