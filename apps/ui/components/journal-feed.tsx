"use client";

import { useEffect, useRef, useState } from "react";
import type { EntityRef } from "@/lib/model";
import { useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { EntityLink } from "./entity-link";
import { EntityBody } from "./entity-view";
import { ErrorLine, pageTitle, sectionTitle } from "./ui";

const PAGE = 5;

function Day({ day }: { day: EntityRef }) {
  return (
    <section className="py-4">
      <h2 className={sectionTitle}>
        <EntityLink id={day.id} className="hover:underline">
          {day.title}
        </EntityLink>
      </h2>
      {/* Each day loads on its own, so one slow or broken page holds up
          neither the days around it nor the top of the feed. */}
      <Region
        pending={<Skeleton className="h-4 w-1/2" />}
        failed={(error) => (
          <ErrorLine>{error.message}</ErrorLine>
        )}
      >
        <EntityBody id={day.id} />
      </Region>
    </section>
  );
}

function Days() {
  const days = useRead((g) => g.journals());
  const [shown, setShown] = useState(PAGE);
  const more = useRef<HTMLDivElement>(null);
  const hasMore = shown < days.length;

  // Reaching the end of what is shown adds the next few days.
  useEffect(() => {
    const el = more.current;
    if (!el) return;
    const watcher = new IntersectionObserver(
      ([entry]) => entry?.isIntersecting && setShown((n) => n + PAGE),
      { rootMargin: "400px" },
    );
    watcher.observe(el);
    return () => watcher.disconnect();
  }, [shown, hasMore]);

  return (
    <>
      {days.slice(0, shown).map((day) => (
        <Day key={day.id} day={day} />
      ))}
      {hasMore ? (
        <div ref={more} className="py-4">
          <Skeleton className="h-4 w-1/3" />
        </div>
      ) : (
        <p className="py-8 text-sm text-ink-muted">That's the lot.</p>
      )}
    </>
  );
}

/** The home page: today's journal, then scroll-back. */
export function JournalFeed() {
  return (
    <article className="mx-auto max-w-measure-text px-6 py-8">
      <h1 className={pageTitle}>Journals</h1>
      <Region
        pending={<Skeleton className="h-8 w-1/3" />}
        failed={(error) => (
          <ErrorLine>{error.message}</ErrorLine>
        )}
      >
        <Days />
      </Region>
    </article>
  );
}
