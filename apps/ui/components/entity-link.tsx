"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import type { ReactNode } from "react";
import type { EntityId } from "@/lib/model";
import { useView } from "./use-view";

/** Click opens in the main pane; shift-click opens at the side. */
export function EntityLink({
  id,
  className,
  title,
  children,
}: {
  id: EntityId;
  className?: string;
  title?: string;
  children: ReactNode;
}) {
  const view = useView();
  const router = useRouter();
  return (
    <Link
      href={view.toMain(id)}
      className={className}
      title={title}
      onClick={(e) => {
        if (e.shiftKey) {
          e.preventDefault();
          router.push(view.toSide(id));
        }
      }}
    >
      {children}
    </Link>
  );
}
