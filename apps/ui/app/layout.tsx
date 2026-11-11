import type { Metadata, Viewport } from "next";
import { Suspense } from "react";
import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-400-italic.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "./globals.css";
import { Skeleton } from "@/components/boundary";
import { ClientOnly } from "@/components/client-only";
import { HistoryKeys } from "@/components/history-keys";
import { LeftSidebar } from "@/components/left-sidebar";
import { RightPane } from "@/components/right-pane";
import { Search } from "@/components/search";
import { SyncIndicator } from "@/components/sync-indicator";
import { Shell } from "@/components/shell";
import { UndoKeys } from "@/components/undo-keys";
import { UpdateNotice } from "@/components/update-notice";

export const metadata: Metadata = { title: "Quiddity" };

/**
 * What the browser, or the system around a webview, paints outside the page (a
 * tab strip, a status bar) is the page's own ground, so the page doesn't end at
 * an edge. These are `--surface` in globals.css, and a test holds them to it.
 */
export const viewport: Viewport = {
  colorScheme: "light dark",
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f8fafc" },
    { media: "(prefers-color-scheme: dark)", color: "#0f172a" },
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
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en-GB">
      <body>
        <ClientOnly>
          <Suspense fallback={null}>
            <UndoKeys />
            <HistoryKeys />
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
          {/* The graph only exists in the browser, so the page isn't prerendered. */}
          <ClientOnly fallback={<Skeleton className="m-8 h-8 w-1/3" />}>
            <Suspense fallback={<Skeleton className="m-8 h-8 w-1/3" />}>{children}</Suspense>
          </ClientOnly>
        </Shell>
      </body>
    </html>
  );
}
