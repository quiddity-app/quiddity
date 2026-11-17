"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useRef } from "react";
import { pageGraph } from "@/lib/page-graph";
import type { EntityId } from "@/lib/model";
import { useRead } from "@/lib/use-graph";
import { draw } from "./graph-view";
import { Region, Skeleton } from "./boundary";
import { useView } from "./use-view";
import { iconButtonClass, paneHeading } from "./ui";

function Canvas({ id }: { id: EntityId }) {
  const all = useRead((g) => g.connections());
  const data = useMemo(() => pageGraph(all, id), [all, id]);
  const view = useView();
  const router = useRouter();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const drawing = useRef<ReturnType<typeof draw> | null>(null);
  // d3 outlives renders, so it is given the latest of these when it needs them.
  const latest = useRef({ view, router });
  latest.current = { view, router };

  useEffect(() => {
    const list = listRef.current!;
    drawing.current = draw(
      canvasRef.current!,
      list,
      (to, aside) => {
        const { view, router } = latest.current;
        router.push(aside ? view.toSide(to) : view.toMain(to));
      },
      (to) => process.env.NEXT_PUBLIC_QUIDDITY_BASE_PATH + latest.current.view.toMain(to),
      // This is a part of the graph, so it keeps to itself: the whole graph's
      // positions are not its own, and its are not the whole graph's.
      { persist: false },
    );
    return () => {
      drawing.current?.stop();
      drawing.current = null;
      list.replaceChildren();
    };
  }, []);

  useEffect(() => {
    drawing.current?.update(data);
    drawing.current?.highlight(id);
  }, [data, id]);

  return (
    <div role="group" aria-label="Entities near this page" className="relative h-72">
      <canvas ref={canvasRef} aria-hidden className="absolute inset-0 size-full touch-none" />
      <div ref={listRef} className="sr-only" />
    </div>
  );
}

/**
 * The right pane's graph of the main entity and what is near it
 * (`lib/page-graph.ts`), with the entity picked out. Its header sits outside
 * the region that suspends so it can always be closed.
 */
export function PageGraph({ id }: { id: EntityId }) {
  const view = useView();
  return (
    <section aria-label="Page graph" className="mx-3 mb-3 rounded-lg border border-border bg-surface">
      <header className="flex items-center gap-1 px-3 py-0.5">
        <h2 className={`${paneHeading} min-w-0 flex-1`}>Page graph</h2>
        <Link href={view.togglePageGraph()} className={iconButtonClass} aria-label="Close page graph">
          ✕
        </Link>
      </header>
      <div className="border-t border-border">
        <Region
          pending={<Skeleton className="m-3 h-64" />}
          failed={(error) => <p className="p-3 text-sm">The graph could not be drawn: {error.message}</p>}
        >
          <Canvas id={id} />
        </Region>
      </div>
    </section>
  );
}
