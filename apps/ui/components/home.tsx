'use client';

import { useEffect } from 'react';

import { useRouter } from 'next/navigation';

import { Skeleton } from './boundary';
import { useView } from './use-view';

import type { Start } from '@/lib/model';

import { useRead } from '@/lib/use-graph';

const paths: Record<Exclude<Start['kind'], 'entity'>, string> = {
  journals: '/journals/',
  all: '/all/',
  graph: '/graph/',
};

// The graph loads before `/` can redirect, and the sidebar's links work from
// the moment it does. A link followed in that gap is the person's choice, and
// the redirect, being later, would overwrite it. So links followed before the
// first redirect are noted (in the capture phase, so nothing can stop them),
// and the redirect stands down. (A navigation that isn't a link says so with
// `noteFollowed`.) It is only the first that can race: after that
// a visit to `/` is a fresh arrival.
let followed = false;
let first = true;

/**
 * Notes a navigation the person chose that isn't a click on a link, such as
 * picking a search result, so that the first redirect stands down for it too.
 */
export function noteFollowed() {
  followed = true;
}

if (typeof document !== 'undefined') {
  document.addEventListener(
    'click',
    (event) => {
      if (event.target instanceof Element && event.target.closest('a[href]'))
        followed = true;
    },
    true,
  );
}

/**
 * `/` goes wherever the graph's `home` setting says, keeping any entities
 * open at the side. It replaces itself in the history, so Back doesn't land
 * here only to be sent on again.
 */
export function Home() {
  const start = useRead((g) => g.start());
  const router = useRouter();
  // `/` has no view of its own to keep but the side pane, which is read the
  // same way as anywhere else.
  const { open, href: toHref } = useView();
  const href =
    start.kind === 'entity'
      ? toHref({ path: '/', main: start.id, open })
      : toHref({ path: paths[start.kind], main: null, open });

  useEffect(() => {
    const raced = first && followed;
    first = false;
    if (!raced) router.replace(href);
  }, [router, href]);
  return <Skeleton className="m-8 h-8 w-1/3" />;
}
