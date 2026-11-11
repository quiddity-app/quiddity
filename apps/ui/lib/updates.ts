/** An update the app has found and can install. */
export type Update = { version: string; notes: string | null };

/** Only the app updates itself: a browser tab is always the latest build. */
export function canUpdate(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Asks the shell whether there is a newer release. There is none to find
 * before the first release, and no network is an ordinary state, so a failed
 * check is the same as nothing found.
 */
export async function checkForUpdate(): Promise<Update | null> {
  if (!canUpdate()) return null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Update | null>("check_for_update");
  } catch (error) {
    console.info("Couldn't check for an update", error);
    return null;
  }
}

/** Installs the update the last check found. The app restarts when it is done. */
export async function installUpdate(): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("install_update");
}
