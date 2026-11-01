import type { Metadata } from "next";
import { Suspense } from "react";
import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-400-italic.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "./globals.css";
import { Skeleton } from "@/components/boundary";
import { ClientOnly } from "@/components/client-only";
import { LeftSidebar } from "@/components/left-sidebar";
import { RightPane } from "@/components/right-pane";
import { Search } from "@/components/search";
import { SyncIndicator } from "@/components/sync-indicator";
import { Shell } from "@/components/shell";
import { UndoKeys } from "@/components/undo-keys";

export const metadata: Metadata = { title: "Quiddity" };

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
          </Suspense>
        </ClientOnly>
        <Shell
          actions={
            <ClientOnly>
              {/* Each waits alone: search takes Ctrl-K before the graph has loaded. */}
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
