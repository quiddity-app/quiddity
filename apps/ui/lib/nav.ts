import type { EntityId } from "./model";

/**
 * The whole view state lives in the URL: `/entity/?id=<main>&open=<a>,<b>`.
 * `/` goes to the graph's home (`components/home.tsx`), `/journals/` is the
 * journals, `/all/` lists every entity and `/graph/` draws them. The right
 * pane is the ordered `open` list.
 */
export interface View {
  /** The page shown when there is no `main` entity: `/journals/`, `/all/` or `/graph/`, or `/` for home. */
  path: string;
  main: EntityId | null;
  open: EntityId[];
}

export function parseView(pathname: string, params: URLSearchParams): View {
  const main = pathname.startsWith("/entity") ? params.get("id") : null;
  const open = (params.get("open") ?? "").split(",").filter(Boolean);
  const path =
    ["/journals/", "/all/", "/graph/"].find((p) => pathname.startsWith(p.slice(0, -1))) ?? "/";
  return { path, main, open: [...new Set(open)] };
}

export function viewHref({ path, main, open }: View): string {
  const params = new URLSearchParams();
  if (main) params.set("id", main);
  if (open.length) params.set("open", open.join(","));
  const query = params.size ? `?${params}` : "";
  return `${main ? "/entity/" : path}${query}`;
}

/** The pages a link in a block's text may name by path, as `[text](/graph/)`. */
const APP_PAGES = ["/journals/", "/all/", "/graph/", "/devices/"];

/** The app page `href` names, as `Link` wants it, or null if it names none. */
export function appPage(href: string | undefined): string | null {
  const page = APP_PAGES.find((p) => href === p || href === p.slice(0, -1));
  return page ?? null;
}
