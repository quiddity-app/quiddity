import type { EntityId } from './model';

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
  /** On `/disambiguate/`, the link whose matches are listed, as written between its brackets. */
  link?: string | null;
  /** Whether the right pane shows the graph around the main entity. */
  pageGraph?: boolean;
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

export function parseView(
  pathname: string,
  params: URLSearchParams,
  names?: Names | null,
): View {
  const named = pathname.startsWith('/page/');
  const mainName = named
    ? decode(pathname.slice('/page/'.length).split('/')[0] ?? '')
    : null;
  let main: string | null = null;
  if (pathname.startsWith('/entity')) {
    main = params.get('id');
  } else if (mainName !== null) {
    main = names?.id(mainName) ?? null;
  }
  // Names are written as they are, so unlike IDs they can't be split on a comma.
  const open = params
    .getAll('open')
    .flatMap((value) => (names ? [value] : value.split(',')))

    .filter(Boolean)
    .map((value) => (names ? (names.id(value) ?? value) : value));
  const path =
    ['/journals/', '/all/', '/graph/', '/disambiguate/'].find((p) =>
      pathname.startsWith(p.slice(0, -1)),
    ) ?? '/';
  const link = path === '/disambiguate/' ? params.get('link') : null;
  return {
    path,
    main,
    open: [...new Set(open)],
    link,
    pageGraph: params.get('pagegraph') === '1',
  };
}

export function viewHref(
  { path, main, open, link, pageGraph }: View,
  names?: Names | null,
): string {
  const params = new URLSearchParams();
  if (main && pageGraph) {
    params.set('pagegraph', '1');
  }
  if (!main && path === '/disambiguate/' && link) {
    params.set('link', link);
  }
  const mainName = main && names?.name(main);
  if (main && !mainName) {
    params.set('id', main);
  }
  if (names) {
    for (const id of open) {
      params.append('open', names.name(id) ?? id);
    }
  } else if (open.length > 0) {
    params.set('open', open.join(','));
  }
  const query = params.size > 0 ? `?${params}` : '';
  if (mainName) {
    return `/page/${encodeURIComponent(mainName)}/${query}`;
  }
  return `${main ? '/entity/' : path}${query}`;
}

/**
 * Fixed places in the app, as opposed to entities in the graph. The sidebar
 * lists the ones in it; the others are in the More menu. The app's Go menu
 * (`apps/app/src-tauri/src/menu.rs`) lists all of them, with these paths.
 */
export const entrypoints = [
  { title: 'Journals', href: '/journals/', in: 'sidebar' },
  { title: 'All entities', href: '/all/', in: 'sidebar' },
  { title: 'Graph', href: '/graph/', in: 'sidebar' },
  { title: 'Devices', href: '/devices/', in: 'menu' },
  { title: 'Stats and debug', href: '/stats/', in: 'menu' },
] as const;

/** The pages a link in a block's text may name by path, as `[text](/graph/)`. */
const APP_PAGES: string[] = entrypoints.map((e) => e.href);

/** The app page `href` names, as `Link` wants it, or null if it names none. */
export function appPage(href: string | undefined): string | null {
  const page = APP_PAGES.find((p) => href === p || href === p.slice(0, -1));
  return page ?? null;
}

/** What an app page is called, as the sidebar and the Go menu call it. */
export function appPageTitle(page: string): string | undefined {
  return entrypoints.find((e) => e.href === page)?.title;
}
