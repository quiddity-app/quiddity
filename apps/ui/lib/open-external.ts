/** Where a link in a block's text may take the user: the web and mail. */
export function isExternal(href: string | undefined): href is string {
  return /^(https?:|mailto:)/i.test(href ?? '');
}

/**
 * Opens a link outside the app. In the app that is the system's browser, since
 * following it in place would replace the UI itself with the page.
 */
export async function openExternal(url: string) {
  if (!isExternal(url)) return;
  if ('__TAURI_INTERNALS__' in window) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}
