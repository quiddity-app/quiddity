"use client";

import { useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import { useGraph } from "@/lib/use-graph";
import type { Property, PropertyValue, ResolvedLink } from "@/lib/model";
import { EntityLink } from "./entity-link";
import { useView } from "./use-view";
import { linkClass } from "./ui";

const unresolved = "cursor-help underline decoration-dashed underline-offset-2";

/**
 * A link to an entity, as the core resolved it. `fallback` is the name to show
 * if the core has no answer for `linkKey`.
 */
export function LinkOrNot({
  linkKey,
  fallback = linkKey,
  links,
  label,
}: {
  linkKey: string;
  fallback?: string;
  links: Record<string, ResolvedLink>;
  /** What to show instead of the link's name, as in `[label]([[Name]])`. */
  label?: ReactNode;
}) {
  const resolved = links[linkKey];
  const target = resolved ?? { kind: "missing" };
  const name = label ?? resolved?.name ?? fallback;
  const qualifiers = resolved?.qualifiers ?? "";
  if (target.kind === "entity") {
    return (
      <EntityLink
        id={target.id}
        className={linkClass}
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
      className={`text-ink-muted ${unresolved}`}
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
  name: ReactNode;
  linkKey: string;
  qualifiers: string;
}) {
  const graph = useGraph();
  const view = useView();
  const router = useRouter();
  return (
    <a
      href="#"
      className={linkClass}
      title={qualifiers || undefined}
      onClick={(e) => {
        e.preventDefault();
        const id = graph.followLink(linkKey);
        router.push(e.shiftKey ? view.toSide(id) : view.toMain(id));
      }}
    >
      {name}
    </a>
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
      return <span className="text-ink-muted">(({value.id}))</span>;
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

/** A link to the entity holding a block, written `[label](((id)))`. */
export function BlockRefLink({ id, children }: { id: string; children: ReactNode }) {
  const graph = useGraph();
  const view = useView();
  const router = useRouter();
  const [missing, setMissing] = useState(false);
  return (
    <button
      type="button"
      className={
        missing
          ? "text-ink-muted cursor-help underline decoration-dashed underline-offset-2"
          : linkClass
      }
      title={missing ? "No block has this id" : "Opens the page holding the block"}
      onClick={(e) => {
        try {
          const entity = graph.followBlockRef(id);
          router.push(e.shiftKey ? view.toSide(entity) : view.toMain(entity));
        } catch {
          setMissing(true);
        }
      }}
    >
      {children}
    </button>
  );
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
    <a
      href="#"
      className={`${className} hover:underline`}
      onClick={(e) => {
        e.preventDefault();
        const id = graph.defineProperty(property.key);
        router.push(e.shiftKey ? view.toSide(id) : view.toMain(id));
      }}
    >
      {children}
    </a>
  );
}

const flaggedChip = "ring-1 ring-flag-ring";

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
      className={`inline-flex items-baseline gap-1 rounded-full bg-surface-hover px-2 py-0.5 text-xs leading-5 ${flagged ? flaggedChip : ""}`}
      title={flagged ? property.flags.join("\n") : undefined}
      data-flagged={flagged || undefined}
    >
      {property.meta ? (
        <>
          <span className="text-ink-muted">##{property.key}</span>
          {property.value?.kind === "text" && <code>{property.value.text}</code>}
        </>
      ) : property.value === null ? (
        <PropertyKey property={property} className="">
          #{property.key}
        </PropertyKey>
      ) : (
        <>
          <PropertyKey property={property} className="text-ink-muted">
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
