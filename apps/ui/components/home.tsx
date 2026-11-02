"use client";

import { useRouter, useSearchParams } from "next/navigation";
import { useEffect } from "react";
import type { Start } from "@/lib/model";
import { viewHref } from "@/lib/nav";
import { useRead } from "@/lib/use-graph";
import { Skeleton } from "./boundary";

const paths: Record<Exclude<Start["kind"], "entity">, string> = {
  journals: "/journals/",
  all: "/all/",
  graph: "/graph/",
};

/**
 * `/` goes wherever the graph's `home` setting says, keeping any entities
 * open at the side. It replaces itself in the history, so Back doesn't land
 * here only to be sent on again.
 */
export function Home() {
  const start = useRead((g) => g.start());
  const params = useSearchParams();
  const router = useRouter();
  const open = (params.get("open") ?? "").split(",").filter(Boolean);
  const href =
    start.kind === "entity"
      ? viewHref({ path: "/", main: start.id, open })
      : viewHref({ path: paths[start.kind], main: null, open });

  useEffect(() => {
    router.replace(href);
  }, [router, href]);
  return <Skeleton className="m-8 h-8 w-1/3" />;
}
