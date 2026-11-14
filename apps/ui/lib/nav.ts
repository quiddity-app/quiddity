import type { EntityId } from "./model";

/**
 * The whole view state lives in the URL: `/entity/?id=<main>&open=<a>,<b>`.
 * `/` goes to the graph's home (`components/home.tsx`), `/journals/` is the
 * journals, `/all/` lists every entity and `/graph/` draws them. The right
 * pane is the ordered `open` list.
 *
 * An ID changes whenever the graph is imported afresh, so a published site
 * (see `Names`) addresses a page by its name instead: `/page/<name>/`, with
 * the side pane's entities named in repeated `open` parameters.
 */
export interface View {
  /** The page shown when there is no `main` entity: `/journals/`, `/all/` or `/graph/`, or `/` for home. */
  path: string;
  main: EntityId | null;
  open: EntityId[];
}

/**
 * The names a published site has pages for. An entity with no page, such as
 * one made in the browser since the build, is addressed by its ID as ever.
 */
export interface Names {
  /** The name of the page `id` has, if it has one. */
  name(id: EntityId): string | undefined;
  /** The entity whose name is `name`. */
  id(name: string): EntityId | undefined;
}

function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

export function parseView(pathname: string, params: URLSearchParams, names?: Names | null): View {
  const named = pathname.startsWith("/page/");
  const mainName = named ? decode((pathname.slice("/page/".length).split("/")[0] ?? "")) : null;
  const main = pathname.startsWith("/entity")
    ? params.get("id")
    : mainName !== null
      ? (names?.id(mainName) ?? null)
      : null;
  // Names are written as they are, so unlike IDs they can't be split on a comma.
  const open = [
    ...params.getAll("open").flatMap((value) => (names ? [value] : value.split(","))),
  ]
    .filter(Boolean)
    .map((value) => (names ? (names.id(value) ?? value) : value));
  const path =
    ["/journals/", "/all/", "/graph/"].find((p) => pathname.startsWith(p.slice(0, -1))) ?? "/";
  return { path, main, open: [...new Set(open)] };
}

export function viewHref({ path, main, open }: View, names?: Names | null): string {
  const params = new URLSearchParams();
  const mainName = main && names?.name(main);
  if (main && !mainName) params.set("id", main);
  if (names) {
    for (const id of open) params.append("open", names.name(id) ?? id);
  } else if (open.length) {
    params.set("open", open.join(","));
  }
  const query = params.size ? `?${params}` : "";
  if (mainName) return `/page/${encodeURIComponent(mainName)}/${query}`;
  return `${main ? "/entity/" : path}${query}`;
}

/** The pages a link in a block's text may name by path, as `[text](/graph/)`. */
const APP_PAGES = ["/journals/", "/all/", "/graph/", "/devices/"];

/** The app page `href` names, as `Link` wants it, or null if it names none. */
export function appPage(href: string | undefined): string | null {
  const page = APP_PAGES.find((p) => href === p || href === p.slice(0, -1));
  return page ?? null;
}
