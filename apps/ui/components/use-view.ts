"use client";

import { usePathname, useSearchParams } from "next/navigation";
import { useMemo } from "react";
import type { EntityId } from "@/lib/model";
import { parseView, viewHref, type Names, type View } from "@/lib/nav";
import { useRead } from "@/lib/use-graph";

/**
 * A published site's names, as written by `quiddity-node names`. Empty in the
 * app, and in a site built without a list.
 */
const PAGES: ReadonlySet<string> = new Set(
  process.env.NEXT_PUBLIC_QUIDDITY_PAGE_NAMES
    ? (JSON.parse(process.env.NEXT_PUBLIC_QUIDDITY_PAGE_NAMES) as string[])
    : [],
);

/**
 * Entities that have a page of their own by name. It is chosen when the site
 * is built, so which hook this is never changes.
 */
function usePublishedNames(): Names {
  const entities = useRead((g) => g.entities());
  return useMemo(() => {
    const byId = new Map<EntityId, string>();
    const byName = new Map<string, EntityId>();
    // A virtual entity (`~name`) has a title but no page, and one made since
    // the build has a page only once the site is built again.
    for (const { id, title } of entities) {
      if (PAGES.has(title) && !id.startsWith("~") && !byName.has(title)) {
        byId.set(id, title);
        byName.set(title, id);
      }
    }
    return { name: (id) => byId.get(id), id: (name) => byName.get(name) };
  }, [entities]);
}

const useNames: () => Names | null = process.env.NEXT_PUBLIC_QUIDDITY_PAGE_NAMES
  ? usePublishedNames
  : () => null;

/** The current view, and hrefs derived from it that keep the rest of it. */
export function useView() {
  const pathname = usePathname();
  const params = useSearchParams();
  const names = useNames();
  const view = useMemo(() => parseView(pathname, params, names), [pathname, params, names]);
  const href = (v: View) => viewHref(v, names);
  return {
    ...view,
    /** The address of any view: the page's name when it has one, else its ID. */
    href,
    toMain: (id: EntityId) => href({ ...view, main: id }),
    toSide: (id: EntityId): string =>
      href({
        ...view,
        open: view.open.includes(id) ? view.open : [...view.open, id],
      }),
    toClosed: (id: EntityId) => href({ ...view, open: view.open.filter((o) => o !== id) }),
    toAllClosed: () => href({ ...view, open: [] }),
    toMainFromSide: (id: EntityId) =>
      href({ ...view, main: id, open: view.open.filter((o) => o !== id) }),
  } satisfies View & Record<string, unknown>;
}
