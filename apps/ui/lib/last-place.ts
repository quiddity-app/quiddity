import { placeAt, type Place } from './history';

/**
 * Where the person was when the page last went out of sight, so that a page
 * that is started over (Android destroys the webview of an app in the
 * background to free memory, and loads it again at `/` when it returns) can
 * put them back rather than at home.
 */
export interface LastPlace {
  /** The whole address: the side panel's cards, and the page graph, as well as the main pane. */
  href: string;
  /** How far the main pane was scrolled. */
  top: number;
}

const KEY = 'quiddity:last-place';

/**
 * How long a place is worth going back to. An app that was put away this
 * morning and opened again this evening starts at home, as a new launch does.
 */
export const RESUME_MS = 6 * 60 * 60 * 1000;

/** The place an address is, if it is one worth going back to (`placeAt`). */
export function placeOf(href: string): Place | undefined {
  const { pathname, search } = new URL(href, 'https://app.invalid');
  return placeAt(pathname, search);
}

export const encode = (last: LastPlace, now: number) =>
  JSON.stringify({ ...last, at: now });

/**
 * What `encode` made, if it is fresh and still reads as a place of this app.
 * Storage is the page's own, but it outlives the build that wrote it, and an
 * address from it is followed, so it is checked and not trusted.
 */
export function decode(
  text: string | null,
  now: number,
): LastPlace | undefined {
  if (text === null) {
    return undefined;
  }
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (typeof value !== 'object' || value === null) {
    return undefined;
  }
  const { href, top, at } = value as Record<string, unknown>;
  if (
    typeof href !== 'string' ||
    typeof top !== 'number' ||
    typeof at !== 'number' ||
    !Number.isFinite(top) ||
    top < 0 ||
    // Not from the future (a clock put back), and not stale.
    !(now - at >= 0 && now - at <= RESUME_MS)
  ) {
    return undefined;
  }
  // An address of this origin only: `//host/path` is another site's.
  if (!href.startsWith('/') || href.startsWith('//') || !placeOf(href)) {
    return undefined;
  }
  return { href, top };
}

/**
 * Only where the graph's entities keep their IDs from one load to the next,
 * which is the app. The in-memory graph of a browser is made afresh each time,
 * so an address from before names nothing; a test plays the app by setting
 * `__quiddityResume`.
 */
const resumes = () =>
  typeof window !== 'undefined' &&
  ('__TAURI_INTERNALS__' in window || '__quiddityResume' in window);

function read(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

/** Read as the module loads, before anything can keep a newer place over it. */
const left = resumes() ? decode(read(), Date.now()) : undefined;

/** What the page was left at before this load, if it should be gone back to. */
export const lastPlace = (): LastPlace | undefined => left;

/** Set once the page is to start afresh (another graph), so nothing is kept again. */
let forgotten = false;

/** Notes where the main pane is, for the next load. */
function keepPlace() {
  const { pathname, search } = window.location;
  // `/` and its like only redirect: leave what was kept before them.
  if (forgotten || !resumes() || !placeAt(pathname, search)) {
    return;
  }
  const pane = document.querySelector('main');
  try {
    localStorage.setItem(
      KEY,
      encode(
        { href: pathname + search, top: pane?.scrollTop ?? 0 },
        Date.now(),
      ),
    );
  } catch {
    // Private mode, or a full disk: starting at home is the fallback.
  }
}

/**
 * Forgets the place, and keeps none until the page is loaded again. A new
 * graph has none of the old one's entities, and the page is about to start
 * over, which would otherwise record where it was on the way out.
 */
export function forgetPlace() {
  forgotten = true;
  try {
    localStorage.removeItem(KEY);
  } catch {
    // Nothing was kept.
  }
}

if (typeof window !== 'undefined') {
  // `visibilitychange` is what an Android activity going away fires, and
  // `pagehide` what leaving the page does anywhere else.
  window.addEventListener('pagehide', keepPlace);
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      keepPlace();
    }
  });
}
