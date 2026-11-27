import { useSyncExternalStore } from 'react';

/** An update the app has found and can install. */
export interface Update {
  version: string;
  notes: string | null;
}

/**
 * Where a check has got to. `current` and `failed` are only said after a check
 * the user asked for: the one made when the app opens is quiet about both,
 * since having no release yet, or no network, is an ordinary state.
 */
export type CheckStatus = 'idle' | 'checking' | 'current' | 'failed';

export interface UpdateState {
  /** Whether this app can update itself: the desktop app, and nothing else. */
  supported: boolean;
  /** What the last check found, until a later one finds nothing. */
  update: Update | null;
  status: CheckStatus;
}

/** How the page asks the shell: `invoke` in the app, something else in a test. */
export type Invoke = <T>(command: string) => Promise<T>;

const initial: UpdateState = { supported: false, update: null, status: 'idle' };

/** How long "up to date" and "couldn't check" stay up before they clear. */
export const NOTE_MS = 4000;

/** Whether the page is inside the app, which a browser tab and the published site aren't. */
function inApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function tauriInvoke<T>(command: string): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command);
}

/**
 * Checks for updates for the page. The state is kept outside React, so that
 * the notice in the header and the menus that ask for a check share it.
 */
export class Updates {
  private state = initial;
  private readonly listeners = new Set<() => void>();
  private clear: ReturnType<typeof setTimeout> | undefined;
  private asking: Promise<void> | undefined;

  constructor(
    private readonly invoke: Invoke,
    private readonly available: () => boolean,
  ) {}

  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  readonly get = () => this.state;

  private set(next: Partial<UpdateState>) {
    this.state = { ...this.state, ...next };
    for (const listener of this.listeners) {
      listener();
    }
  }

  /**
   * Asks the shell whether there is a newer release. A check already under
   * way is joined, not repeated. `manual` is a check the user asked for,
   * which says what it found even when that is nothing, or an error.
   */
  check(manual: boolean): Promise<void> {
    if (this.asking) {
      return this.asking;
    }
    const asking = this.run(manual).finally(() => {
      this.asking = undefined;
    });
    this.asking = asking;
    return asking;
  }

  private async run(manual: boolean) {
    if (!this.available()) {
      return;
    }
    clearTimeout(this.clear);
    this.set({ status: manual ? 'checking' : 'idle' });
    try {
      // Asked of the shell rather than guessed from the platform: Android is
      // an app too, and can't replace itself.
      if (!(await this.invoke<boolean>('updates_supported'))) {
        this.set({ status: 'idle' });
        return;
      }
      this.set({ supported: true });
      const update = await this.invoke<Update | null>('check_for_update');
      this.set({ update, status: manual && !update ? 'current' : 'idle' });
    } catch (error) {
      console.info("Couldn't check for an update", error);
      this.set({ status: manual ? 'failed' : 'idle' });
    }
    if (this.state.status === 'current' || this.state.status === 'failed') {
      this.clear = setTimeout(() => {
        this.set({ status: 'idle' });
      }, NOTE_MS);
    }
  }
}

const updates = new Updates(tauriInvoke, inApp);

/** The page's update state, and what has been found. */
export function useUpdates(): UpdateState {
  return useSyncExternalStore(updates.subscribe, updates.get, () => initial);
}

/** Looks for an update, quietly: the check made when the app opens. */
export function checkForUpdate(): Promise<void> {
  return updates.check(false);
}

/** Looks for an update because the user asked, and says what it found. */
export function checkForUpdateNow(): Promise<void> {
  return updates.check(true);
}

/** Installs the update the last check found. The app restarts when it is done. */
export async function installUpdate(): Promise<void> {
  await tauriInvoke('install_update');
}
