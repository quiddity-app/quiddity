import { entrypoints } from './nav';

/**
 * What the app's native menu asks of the page. The shell sends one string on
 * the `menu` event (`apps/app/src-tauri/src/menu.rs`): `delete-entity`, or
 * `go:<path>` for one of the entrypoints. In a browser there is no menu.
 */
export type MenuCommand =
  { kind: 'delete-entity' } | { kind: 'go'; path: string };

/** Whether the page is running inside the app, which has the menu. */
function inApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Reads what the shell sent. Anything else, including a path that isn't an entrypoint, is ignored. */
export function parseMenuCommand(payload: unknown): MenuCommand | null {
  if (payload === 'delete-entity') return { kind: 'delete-entity' };
  if (typeof payload !== 'string' || !payload.startsWith('go:')) return null;
  const path = payload.slice('go:'.length);
  return entrypoints.some((e) => e.href === path) ? { kind: 'go', path } : null;
}

/** Calls `handler` for each command from the menu, until the returned function is called. */
export function listenToMenu(
  handler: (command: MenuCommand) => void,
): () => void {
  if (!inApp()) return () => {};
  let stop: (() => void) | undefined;
  let stopped = false;
  void import('@tauri-apps/api/event')
    .then(({ listen }) =>
      listen<unknown>('menu', (event) => {
        const command = parseMenuCommand(event.payload);
        if (command) handler(command);
      }),
    )
    .then((unlisten) => {
      // Stopped while still subscribing.
      if (stopped) unlisten();
      else stop = unlisten;
    })
    .catch((error) => console.info("Couldn't listen to the menu", error));
  return () => {
    stopped = true;
    stop?.();
  };
}

/**
 * Tells the shell whether an entity is open, which is what Delete Entity in
 * the File menu needs to be on. A failure leaves the item as it was.
 */
export function setEntityOpen(open: boolean) {
  if (!inApp()) return;
  void import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('set_entity_open', { open }))
    .catch((error) => console.info("Couldn't update the menu", error));
}
