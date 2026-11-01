"use client";

import type { ReactNode } from "react";
import { useRouter } from "next/navigation";
import { useGraph } from "@/lib/use-graph";
import type { Property, PropertyValue, ResolvedLink } from "@/lib/model";
import { EntityLink } from "./entity-link";
import { useView } from "./use-view";

const unresolved = "cursor-help underline decoration-dashed underline-offset-2";

/**
 * A link to an entity, as the core resolved it. `fallback` is the name to show
 * if the core has no answer for `linkKey`.
 */
export function LinkOrNot({
  linkKey,
  fallback = linkKey,
  links,
}: {
  linkKey: string;
  fallback?: string;
  links: Record<string, ResolvedLink>;
}) {
  const resolved = links[linkKey];
  const target = resolved ?? { kind: "missing" };
  const name = resolved?.name ?? fallback;
  const qualifiers = resolved?.qualifiers ?? "";
  if (target.kind === "entity") {
    return (
      <EntityLink
        id={target.id}
        className="text-sky-700 underline-offset-2 hover:underline dark:text-sky-300"
        title={qualifiers || undefined}
      >
        {name}
      </EntityLink>
    );
  }
  if (target.kind === "missing") {
    return <NewEntityLink name={name} linkKey={linkKey} qualifiers={qualifiers} />;
  }
  return (
    <span
      className={`text-slate-500 dark:text-slate-400 ${unresolved}`}
      title={qualifiers ? `Several entities match ${qualifiers}` : "Several entities match"}
    >
      {name}
    </span>
  );
}

/** A link to an entity that doesn't exist yet. Following it makes the entity. */
function NewEntityLink({
  name,
  linkKey,
  qualifiers,
}: {
  name: string;
  linkKey: string;
  qualifiers: string;
}) {
  const graph = useGraph();
  const view = useView();
  const router = useRouter();
  return (
    <button
      type="button"
      className="text-sky-700 underline decoration-dashed underline-offset-2 hover:decoration-solid dark:text-sky-300"
      title={`No such entity yet. Open it to make it.${qualifiers ? ` ${qualifiers}` : ""}`}
      onClick={(e) => {
        const id = graph.followLink(linkKey);
        router.push(e.shiftKey ? view.toSide(id) : view.toMain(id));
      }}
    >
      {name}
    </button>
  );
}

export function Value({
  value,
  links,
}: {
  value: PropertyValue;
  links: Record<string, ResolvedLink>;
}) {
  switch (value.kind) {
    case "text":
      return <>{value.text}</>;
    case "link":
      return <LinkOrNot linkKey={value.key} fallback={value.name} links={links} />;
    case "ref":
      return <span className="text-slate-500 dark:text-slate-400">(({value.id}))</span>;
    case "list":
      return (
        <>
          {value.items.map((item, i) => (
            <span key={i}>
              {i > 0 && ", "}
              <Value value={item} links={links} />
            </span>
          ))}
        </>
      );
  }
}

/** A property's key, which goes to the entity that defines the property. */
function PropertyKey({
  property,
  className,
  children,
}: {
  property: Property;
  className: string;
  children: ReactNode;
}) {
  const graph = useGraph();
  const view = useView();
  const router = useRouter();
  if (property.definition !== null) {
    return (
      <EntityLink id={property.definition} className={`${className} hover:underline`}>
        {children}
      </EntityLink>
    );
  }
  // Nobody has defined it yet, so following it does.
  return (
    <button
      type="button"
      className={`${className} underline decoration-dashed underline-offset-2 hover:decoration-solid`}
      title="Not defined yet. Open it to define it."
      onClick={(e) => {
        const id = graph.defineProperty(property.key);
        router.push(e.shiftKey ? view.toSide(id) : view.toMain(id));
      }}
    >
      {children}
    </button>
  );
}

const flaggedChip = "ring-1 ring-amber-500 dark:ring-amber-400";

/**
 * A property, as a pill: the key, then its value if it has one. A
 * meta-property shows `##key` and its parameter as written. A chip whose
 * property doesn't fit its schema is ringed, and says why on hover.
 */
export function PropertyChip({
  property,
  links,
}: {
  property: Property;
  links: Record<string, ResolvedLink>;
}) {
  const flagged = property.flags.length > 0;
  return (
    <span
      className={`inline-flex items-baseline gap-1 rounded-full bg-slate-200 px-2 py-0.5 text-xs leading-5 dark:bg-slate-700 ${flagged ? flaggedChip : ""}`}
      title={flagged ? property.flags.join("\n") : undefined}
      data-flagged={flagged || undefined}
    >
      {property.meta ? (
        <>
          <span className="text-slate-500 dark:text-slate-400">##{property.key}</span>
          {property.value?.kind === "text" && <code>{property.value.text}</code>}
        </>
      ) : property.value === null ? (
        <PropertyKey property={property} className="">
          #{property.key}
        </PropertyKey>
      ) : (
        <>
          <PropertyKey property={property} className="text-slate-500 dark:text-slate-400">
            {property.key}
          </PropertyKey>
          <span>
            <Value value={property.value} links={links} />
          </span>
        </>
      )}
    </span>
  );
}
