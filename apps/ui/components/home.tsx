'use client';

import { useEffect, useState } from 'react';

import { useRouter } from 'next/navigation';

import { Skeleton } from './boundary';
import { useView } from './use-view';

import type { Graph } from '@/lib/graph';
import type { Start } from '@/lib/model';

import { lastPlace, placeOf } from '@/lib/last-place';
import { useGraph, useRead } from '@/lib/use-graph';

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
      if (event.target instanceof Element && event.target.closest('a[href]')) {
        followed = true;
      }
    },
    true,
  );
}

/**
 * Where the page was left, if this is a page loaded again after that (Android
 * loads the webview afresh when an app comes back from the background), unless
 * its entity has gone since. Worked out once, so the same answer is given if
 * the redirect is made twice.
 */
let resumed: { href: string | undefined } | undefined;

function resumeAt(graph: Graph): string | undefined {
  resumed ??= (() => {
    const left = lastPlace();
    const main = left && placeOf(left.href)?.main;
    try {
      if (main) {
        graph.entity(main);
      }
    } catch {
      return { href: undefined };
    }
    return { href: left?.href };
  })();
  return resumed.href;
}

/**
 * `/` goes wherever the graph's `home` setting says, keeping any entities
 * open at the side. It replaces itself in the history, so Back doesn't land
 * here only to be sent on again.
 */
export function Home() {
  const start = useRead((g) => g.start());
  const router = useRouter();
  const graph = useGraph();
  // Only the page's first arrival at `/` can be a page loaded again: a later
  // visit (to Home, say) goes home. Read in the render, which is before the
  // effect below says the first has been, so a repeated effect agrees.
  const [loaded] = useState(first);
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
    if (!raced) {
      router.replace((loaded ? resumeAt(graph) : undefined) ?? href);
    }
  }, [router, href, graph, loaded]);
  return <Skeleton className="m-8 h-8 w-1/3" />;
}
