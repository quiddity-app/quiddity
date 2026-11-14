import { readFileSync } from "node:fs";
import { EntityPage } from "@/components/entity-page";

// A published site's pages by name: `/page/<name>/`, which, unlike
// `/entity/?id=`, a rebuild leaves where it was. The names are known at build
// time, from the list written by `quiddity-node names`; the entity is found in
// the browser, as ever. The app has no such list and so no such pages: this
// file is a route only in a published build (`pageExtensions` in
// `next.config.ts`).
export const dynamicParams = false;

export function generateStaticParams(): { name: string }[] {
  return readFileSync(process.env.QUIDDITY_STATIC_NAMES ?? "", "utf8")
    .split("\n")
    .filter(Boolean)
    .map((name) => ({ name }));
}

export default function NamedPage() {
  return <EntityPage />;
}
