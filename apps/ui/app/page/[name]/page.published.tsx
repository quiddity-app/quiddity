import { readFileSync } from 'node:fs';

import type { Metadata } from 'next';

import { EntityPage } from '@/components/entity-page';

// A published site's pages by name: `/page/<name>/`, which, unlike
// `/entity/?id=`, a rebuild leaves where it was. The names are known at build
// time, from the list written by `quiddity-node names`; the entity is found in
// the browser, as ever. The app has no such list and so no such pages: this
// file is a route only in a published build (`pageExtensions` in
// `next.config.ts`).
//
// The page is drawn here too, from the graph being published, so that its
// content is in the HTML and not only in what the browser makes of the
// snapshot (`bootGraph`). The browser then takes over what was drawn.
export const dynamicParams = false;

export function generateStaticParams(): { name: string }[] {
  return readFileSync(process.env.QUIDDITY_STATIC_NAMES ?? '', 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((name) => ({ name }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ name: string }>;
}): Promise<Metadata> {
  const { name } = await params;
  return { title: `${decodeURIComponent(name)} – Quiddity` };
}

export default function NamedPage() {
  return <EntityPage />;
}
