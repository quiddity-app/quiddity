'use client';

import { useSyncExternalStore, type ReactNode } from 'react';

const subscribe = () => () => {};

/** Renders `fallback` while prerendering, and its children in the browser. The
 * graph only exists there. */
export function ClientOnly({
  fallback,
  children,
}: Readonly<{
  fallback?: ReactNode;
  children: ReactNode;
}>) {
  const client = useSyncExternalStore(
    subscribe,
    () => true,
    () => false,
  );
  return client ? children : fallback;
}
