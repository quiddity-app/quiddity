"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { EntityRef } from "@/lib/model";
import { useRecents } from "@/lib/recents";
import { useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { EntityLink } from "./entity-link";
import { useView } from "./use-view";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="px-2 py-2">
      <h2 className="px-2 pb-1 text-xs font-medium uppercase tracking-wide text-slate-500 dark:text-slate-400">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Items({ items }: { items: EntityRef[] }) {
  const { main } = useView();
  if (items.length === 0) {
    return <p className="px-2 text-sm text-slate-500 dark:text-slate-400">Nothing yet.</p>;
  }
  return (
    <ul>
      {items.map((item) => (
        <li key={item.id}>
          <EntityLink
            id={item.id}
            className={`block truncate rounded px-2 py-1 text-sm hover:bg-slate-200 dark:hover:bg-slate-700 ${
              item.id === main ? "bg-slate-200 font-medium dark:bg-slate-700" : ""
            }`}
          >
            {item.title}
          </EntityLink>
        </li>
      ))}
    </ul>
  );
}

const listSkeleton = (
  <div className="space-y-2 px-2">
    <Skeleton className="h-4 w-3/4" />
    <Skeleton className="h-4 w-1/2" />
  </div>
);

function Favourites() {
  return <Items items={useRead((g) => g.favourites())} />;
}

/** Fixed places in the app, as opposed to entities in the graph. */
const entrypoints = [
  { title: "Journals", href: "/" },
  { title: "All entities", href: "/all/" },
  { title: "Graph", href: "/graph/" },
  { title: "Devices", href: "/devices/" },
];

function Entrypoints() {
  const pathname = usePathname();
  const { main } = useView();
  return (
    <ul>
      {entrypoints.map(({ title, href }) => {
        const here = !main && (href === "/" ? pathname === "/" : pathname.startsWith(href));
        return (
          <li key={href}>
            <Link
              href={href}
              className={`block truncate rounded px-2 py-1 text-sm hover:bg-slate-200 dark:hover:bg-slate-700 ${
                here ? "bg-slate-200 font-medium dark:bg-slate-700" : ""
              }`}
            >
              {title}
            </Link>
          </li>
        );
      })}
    </ul>
  );
}

/** Recents are kept on this device, so they can outlive what they point at
 * (a different graph, an undone creation): only those still there are shown,
 * under their current names. */
function Recents() {
  const recents = useRecents();
  const names = useRead((g) => new Map(g.entities().map((e) => [e.id, e.title])));
  const items = recents.flatMap((r) => {
    const title = names.get(r.id);
    return title === undefined ? [] : [{ id: r.id, title }];
  });
  return <Items items={items} />;
}

export function LeftSidebar() {
  const failed = () => (
    <p className="px-2 text-sm text-slate-500 dark:text-slate-400">Couldn't load.</p>
  );
  return (
    <nav aria-label="Navigation" className="overflow-y-auto">
      <Section title="Entrypoints">
        <Entrypoints />
      </Section>
      <Section title="Favourites">
        <Region pending={listSkeleton} failed={failed}>
          <Favourites />
        </Region>
      </Section>
      <Section title="Recents">
        <Region pending={listSkeleton} failed={failed}>
          <Recents />
        </Region>
      </Section>
    </nav>
  );
}
