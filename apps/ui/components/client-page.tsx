import { Suspense, type ReactNode } from "react";
import { Skeleton } from "./boundary";
import { ClientOnly } from "./client-only";

/**
 * A page that needs the graph, which only exists in the browser, and so isn't
 * prerendered: the skeleton stands in until it has loaded. The named pages of
 * a published site (`app/page/[name]`) don't use this, since they are
 * rendered at export time, from the graph being published.
 */
export function ClientPage({ children }: { children: ReactNode }) {
  const skeleton = <Skeleton className="m-8 h-8 w-1/3" />;
  return (
    <ClientOnly fallback={skeleton}>
      <Suspense fallback={skeleton}>{children}</Suspense>
    </ClientOnly>
  );
}
