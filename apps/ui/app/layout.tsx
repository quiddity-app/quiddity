import { Suspense } from 'react';

import type { Metadata, Viewport } from 'next';

import '@fontsource/ibm-plex-sans/latin-400.css';
import '@fontsource/ibm-plex-sans/latin-400-italic.css';
import '@fontsource/ibm-plex-sans/latin-500.css';
import '@fontsource/ibm-plex-sans/latin-600.css';
import './globals.css';
import { Skeleton } from '@/components/boundary';
import { ClientOnly } from '@/components/client-only';
import { HistoryKeys } from '@/components/history-keys';
import { Recovery } from '@/components/recovery';
import { LinkStatus } from '@/components/link-status';
import { LeftSidebar } from '@/components/left-sidebar';
import { MoreMenu } from '@/components/more-menu';
import { RightPane } from '@/components/right-pane';
import { ScrollRestore } from '@/components/scroll-restore';
import { Search } from '@/components/search';
import { SyncIndicator } from '@/components/sync-indicator';
import { Shell } from '@/components/shell';
import { UndoKeys } from '@/components/undo-keys';
import { UpdateNotice } from '@/components/update-notice';

export const metadata: Metadata = { title: 'Quiddity' };

/**
 * What the browser, or the system around a webview, paints outside the page (a
 * tab strip, a status bar) is the page's own ground, so the page doesn't end at
 * an edge. These are `--surface` in globals.css, and a test holds them to it.
 */
export const viewport: Viewport = {
  colorScheme: 'light dark',
  themeColor: [
    { media: '(prefers-color-scheme: light)', color: '#f8fafc' },
    { media: '(prefers-color-scheme: dark)', color: '#0f172a' },
  ],
};

/**
 * Both sidebars read the URL, which suspends up to the nearest boundary, so
 * each pane has its own, and none can hold up another; the right pane's
 * fallback is empty so it reserves no space.
 *
 * They also read the graph, which only exists in the browser, so they render
 * only there (`ClientOnly`). A static export never renders them on a server
 * anyway, but `next dev` does, on every request.
 */
export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en-GB">
      <body>
        <ClientOnly>
          <Recovery />
          <Suspense fallback={null}>
            <UndoKeys />
            <HistoryKeys />
            <ScrollRestore />
          </Suspense>
        </ClientOnly>
        <Shell
          actions={
            <ClientOnly>
              {/* Each waits alone: search takes Ctrl-K before the graph has loaded. */}
              <UpdateNotice />
              <Suspense fallback={null}>
                <SyncIndicator />
              </Suspense>
              <Suspense fallback={null}>
                <Search />
              </Suspense>
              <Suspense fallback={null}>
                <MoreMenu />
              </Suspense>
            </ClientOnly>
          }
          left={
            <ClientOnly fallback={<Skeleton className="m-4 h-4 w-3/4" />}>
              <Suspense fallback={<Skeleton className="m-4 h-4 w-3/4" />}>
                <LeftSidebar />
              </Suspense>
            </ClientOnly>
          }
          right={
            <ClientOnly>
              <Suspense fallback={null}>
                <RightPane />
              </Suspense>
            </ClientOnly>
          }
        >
          {/* The graph only exists in the browser, so each page waits for it
              (`ClientPage`), except the named pages of a published site, which
              are drawn from its graph when it is exported. */}
          {children}
        </Shell>
        <LinkStatus />
      </body>
    </html>
  );
}
