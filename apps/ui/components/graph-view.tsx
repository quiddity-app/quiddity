"use client";

import { drag, type D3DragEvent } from "d3-drag";
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from "d3-force";
import { pointer, select, type Selection } from "d3-selection";
import "d3-transition";
import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import { useRouter } from "next/navigation";
import { useEffect, useRef } from "react";
import { fitTransform, largestGroup } from "@/lib/graph-fit";
import { cachedPosition, cachedTransform, remember } from "@/lib/graph-layout";
import type { Connections } from "@/lib/model";
import { useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";
import { useView } from "./use-view";

interface Node extends SimulationNodeDatum {
  id: string;
  title: string;
  degree: number;
}

type Link = SimulationLinkDatum<Node>;

const radius = (n: Node) => 4 + Math.sqrt(n.degree) * 2;

/** A label is drawn once its node's circle is this many pixels across the
 * radius on screen. At normal zoom that is every node; zoomed out, the hubs
 * keep their names and the leaves lose them. */
const LABEL_RADIUS = 4;

/** The Tailwind colours the SVG version used, which a canvas can't take from
 * classes. */
const palette = {
  light: { link: "#cbd5e1", fill: "#0284c7", stroke: "#0c4a6e", text: "#1e293b" },
  dark: { link: "#475569", fill: "#38bdf8", stroke: "#e0f2fe", text: "#e2e8f0" },
};

/** How the graph draws itself, kept apart from React. */
interface Drawing {
  update(data: Connections): void;
  /** Frames the main body of the graph. */
  fit(): void;
  stop(): void;
}

/**
 * Draws on `canvas`, with `list` holding a real link for each entity. The
 * canvas is what is seen, and is much cheaper than an element per node and
 * edge to move on every tick; the links are for keyboards and screen readers
 * and are not positioned, so they cost nothing while the layout runs. Both
 * are owned here rather than by React.
 */
function draw(
  canvas: HTMLCanvasElement,
  list: HTMLElement,
  open: (id: string, aside: boolean) => void,
  hrefOf: (id: string) => string,
): Drawing {
  const root = select(canvas);
  const dark = matchMedia("(prefers-color-scheme: dark)");

  let nodes: Node[] = [];
  let links: Link[] = [];
  let edges: Connections["edges"] = [];
  let view: ZoomTransform = zoomIdentity;
  const simulation: Simulation<Node, Link> = forceSimulation<Node>()
    // d3's default strength, 1 / the fewer links of the two ends: a spring
    // between two hubs is slack, so hubs don't drag each other into a knot.
    .force("link", forceLink<Node, Link>().id((n) => n.id).distance(50))
    .force("charge", forceManyBody<Node>().strength(-90).distanceMax(400))
    .force("collide", forceCollide<Node>().radius((n) => radius(n) + 2))
    // Loose islands drift towards the middle rather than away for good.
    .force("x", forceX<Node>().strength(0.04))
    .force("y", forceY<Node>().strength(0.04));

  let neighbours = new Map<string, Set<string>>();
  let focused: string | null = null;
  let dragged: Node | null = null;
  let moved = false;
  let ratio = 1;
  let frame = 0;

  // Repaints at most once per animation frame, however often it is asked, and
  // not at all while the page is hidden: the layout carries on, the GPU doesn't.
  const paint = () => {
    frame = 0;
    if (document.hidden) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const colours = dark.matches ? palette.dark : palette.light;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(ratio * view.k, 0, 0, ratio * view.k, ratio * view.x, ratio * view.y);

    const near = focused === null ? null : (neighbours.get(focused) ?? new Set<string>());
    const touches = (l: Link) =>
      (l.source as Node).id === focused || (l.target as Node).id === focused;

    // Links in one path each, dimmed when something is focused, then those
    // that touch the focus on top at full strength.
    const stroke = (wanted: (l: Link) => boolean, alpha: number) => {
      ctx.beginPath();
      for (const l of links) {
        const { source, target } = l as { source: Node; target: Node };
        if (!wanted(l) || source.x === undefined || target.x === undefined) continue;
        ctx.moveTo(source.x, source.y ?? 0);
        ctx.lineTo(target.x, target.y ?? 0);
      }
      ctx.globalAlpha = alpha;
      ctx.stroke();
    };
    ctx.strokeStyle = colours.link;
    ctx.lineWidth = 1;
    stroke(() => true, focused === null ? 1 : 0.1);
    if (focused !== null) stroke(touches, 1);

    ctx.fillStyle = colours.fill;
    ctx.strokeStyle = colours.stroke;
    for (const n of nodes) {
      if (n.x === undefined) continue;
      ctx.globalAlpha = near && n.id !== focused && !near.has(n.id) ? 0.25 : 1;
      ctx.lineWidth = n.id === focused ? 2 : 1;
      ctx.beginPath();
      ctx.arc(n.x, n.y ?? 0, radius(n), 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }

    ctx.font = "10px ui-sans-serif, system-ui, sans-serif";
    ctx.textBaseline = "middle";
    ctx.fillStyle = colours.text;
    for (const n of nodes) {
      if (n.x === undefined) continue;
      if (n.id !== focused && radius(n) * view.k < LABEL_RADIUS) continue;
      ctx.globalAlpha = near && n.id !== focused && !near.has(n.id) ? 0.25 : 1;
      ctx.fillText(n.title, n.x + radius(n) + 3, n.y ?? 0);
    }
    ctx.globalAlpha = 1;
  };
  const repaint = () => {
    if (frame === 0) frame = requestAnimationFrame(paint);
  };
  dark.addEventListener("change", repaint);

  const resize = () => {
    const { width, height } = canvas.getBoundingClientRect();
    ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    repaint();
  };
  const observer = new ResizeObserver(resize);
  observer.observe(canvas);
  resize();

  // Nothing is saved before there is something to save: setting the initial
  // zoom ends a zoom, which would otherwise forget the layout it is about to
  // restore.
  const save = () => {
    if (nodes.length > 0) remember(nodes, view);
  };
  // Entities placed by this layout run and not yet settled: they are the only
  // ones free to move.
  const unsettled = new Set<string>();
  const release = () => {
    for (const n of nodes) {
      n.fx = null;
      n.fy = null;
    }
  };

  let items: Selection<HTMLAnchorElement, Node, HTMLElement, unknown> | undefined;
  // The links carry where their entity is, but only once the layout is still:
  // `settled` says when. Nothing in the page reads them, but tests do.
  const settle = (still: boolean) => {
    canvas.dataset.settled = String(still);
    if (!still) return;
    items?.attr("data-x", (n) => n.x ?? null).attr("data-y", (n) => n.y ?? null);
  };

  simulation.on("tick", repaint);
  simulation.on("end", () => {
    unsettled.clear();
    release();
    // With no remembered view, frame the layout once it has found its shape,
    // unless the person has already taken the view into their own hands.
    if (tracking) {
      tracking = false;
      fit(null, true);
    }
    save();
    settle(true);
    repaint();
  });

  // True while the view is ours to frame: no remembered view, and no pan or zoom yet.
  let tracking = cachedTransform() === null;

  /** The entity under a point in the canvas's own pixels, if any. */
  const nodeAt = ([px, py]: [number, number]): Node | undefined => {
    const x = (px - view.x) / view.k;
    const y = (py - view.y) / view.k;
    let best: Node | undefined;
    let nearest = Infinity;
    for (const n of nodes) {
      if (n.x === undefined || n.y === undefined) continue;
      const away = Math.hypot(n.x - x, n.y - y);
      if (away < radius(n) + 3 / view.k && away < nearest) {
        nearest = away;
        best = n;
      }
    }
    return best;
  };

  const behaviour: ZoomBehavior<HTMLCanvasElement, unknown> = zoom<HTMLCanvasElement, unknown>()
    .scaleExtent([0.1, 4])
    // A press on a node is a drag of the node, not a pan.
    .filter((event: Event) => {
      if ((event.type === "mousedown" || event.type === "touchstart") && nodeAt(pointer(event, canvas))) {
        return false;
      }
      return (!(event as MouseEvent).ctrlKey || event.type === "wheel") && !(event as MouseEvent).button;
    })
    .on("zoom", (event: { transform: ZoomTransform; sourceEvent?: unknown }) => {
      if (event.sourceEvent) tracking = false;
      view = event.transform;
      canvas.dataset.view = `${view.x},${view.y},${view.k}`;
      repaint();
    })
    .on("end", save);
  root.call(behaviour);
  root.on("dblclick.zoom", null);
  const { width, height } = canvas.getBoundingClientRect();
  const start = cachedTransform();
  root.call(
    behaviour.transform,
    start
      ? zoomIdentity.translate(start.x, start.y).scale(start.k)
      : zoomIdentity.translate(width / 2, height / 2),
  );

  /** Frames these entities (the main body of the graph if none are given). */
  const fit = (ids: Set<string> | null, animate: boolean) => {
    const wanted = ids ?? largestGroup(nodes.map((n) => n.id), edges);
    const box = canvas.getBoundingClientRect();
    const target = fitTransform(
      nodes
        .filter((n) => wanted.has(n.id) && n.x !== undefined && n.y !== undefined)
        .map((n) => ({ x: n.x!, y: n.y!, r: radius(n) })),
      box.width,
      box.height,
      // Zooming in on a few is welcome; blowing the whole graph up isn't.
      ids ? { pad: 0.3, minScale: 0.1, maxScale: 2.5 } : { pad: 0.08, minScale: 0.1, maxScale: 1 },
    );
    if (!target) return;
    const to = zoomIdentity.translate(target.x, target.y).scale(target.k);
    if (animate) root.transition().duration(600).call(behaviour.transform, to);
    else root.call(behaviour.transform, to);
  };
  root.on("dblclick.fit", (event: MouseEvent) => {
    if (!nodeAt(pointer(event, canvas))) fit(null, true);
  });

  const focus = (id: string | null) => {
    if (focused === id) return;
    focused = id;
    repaint();
  };
  root.on("mousemove.focus", (event: MouseEvent) => {
    if (dragged) return;
    const n = nodeAt(pointer(event, canvas));
    canvas.style.cursor = n ? "pointer" : "";
    focus(n?.id ?? null);
  });
  root.on("mouseleave.focus", () => {
    if (!dragged) focus(null);
  });
  // A drag that doesn't move ends in a click; one that does, doesn't.
  root.on("click.open", (event: MouseEvent) => {
    const n = nodeAt(pointer(event, canvas));
    if (!n) return;
    // Alt-click looks closer instead of opening.
    if (event.altKey) fit(new Set([n.id, ...(neighbours.get(n.id) ?? [])]), true);
    else open(n.id, event.shiftKey);
  });

  // d3-drag reports a drag in the subject's own coordinates: the subject's
  // position plus how far the pointer has moved. So the subject is where the
  // node is in the canvas's pixels, not its place in the graph's space, which
  // is what makes the node stay under the pointer where it was grabbed.
  interface Grab {
    x: number;
    y: number;
    node: Node;
  }
  type Drag = D3DragEvent<HTMLCanvasElement, unknown, Grab>;
  const dragging = drag<HTMLCanvasElement, unknown, Grab | undefined>()
    .container(canvas)
    .subject((event: D3DragEvent<HTMLCanvasElement, unknown, Grab | undefined>) => {
      const node = nodeAt([event.x, event.y]);
      return node && { x: (node.x ?? 0) * view.k + view.x, y: (node.y ?? 0) * view.k + view.y, node };
    })
    // Pressing starts a drag, but nothing is disturbed until the pointer
    // moves: a click that stays on the graph (Alt, or Shift) must not stir it.
    .on("start", (event: Drag) => {
      dragged = event.subject.node;
      moved = false;
      focus(dragged.id);
    })
    .on("drag", (event: Drag) => {
      const n = event.subject.node;
      if (!moved) {
        moved = true;
        settle(false);
        simulation.alphaTarget(0.2).restart();
        // Let everything go, so the rest can untangle around the dragged node.
        release();
      }
      n.fx = (event.x - view.x) / view.k;
      n.fy = (event.y - view.y) / view.k;
      canvas.dataset.grabbed = `${event.x},${event.y}`;
    })
    .on("end", (event: Drag) => {
      if (moved) simulation.alphaTarget(0);
      event.subject.node.fx = null;
      event.subject.node.fy = null;
      dragged = null;
      delete canvas.dataset.grabbed;
    });
  root.call(dragging);

  return {
    update(data) {
      const known = new Map(nodes.map((n) => [n.id, n]));
      const degree = new Map<string, number>();
      neighbours = new Map();
      for (const { source, target } of data.edges) {
        degree.set(source, (degree.get(source) ?? 0) + 1);
        degree.set(target, (degree.get(target) ?? 0) + 1);
        (neighbours.get(source) ?? neighbours.set(source, new Set()).get(source)!).add(target);
        (neighbours.get(target) ?? neighbours.set(target, new Set()).get(target)!).add(source);
      }
      nodes = data.nodes.map(({ id, title }) => {
        const node: Node = known.get(id) ?? { id, title, degree: 0 };
        node.title = title;
        node.degree = degree.get(id) ?? 0;
        if (node.x === undefined || node.y === undefined) {
          const remembered = cachedPosition(id);
          if (remembered) {
            node.x = remembered.x;
            node.y = remembered.y;
          }
        }
        return node;
      });
      // A new entity starts beside one it links to, if that has a place, or
      // near the middle, so it doesn't fly in from nowhere and shove the rest.
      const byId = new Map(nodes.map((n) => [n.id, n]));
      for (const n of nodes) {
        if (n.x !== undefined && n.y !== undefined) continue;
        const friend = [...(neighbours.get(n.id) ?? [])]
          .map((id) => byId.get(id))
          .find((f) => f?.x !== undefined && f?.y !== undefined);
        const angle = Math.random() * Math.PI * 2;
        n.x = (friend?.x ?? 0) + Math.cos(angle) * 30;
        n.y = (friend?.y ?? 0) + Math.sin(angle) * 30;
        unsettled.add(n.id);
      }
      edges = data.edges;
      links = data.edges.map(({ source, target }) => ({ source, target }));

      items = select(list)
        .selectAll<HTMLAnchorElement, Node>("a")
        .data(nodes, (n) => n.id)
        .join((enter) =>
          enter
            .append("a")
            .on("click", (event: MouseEvent, n) => {
              event.preventDefault();
              open(n.id, event.shiftKey);
            })
            .on("focus", (_event, n) => focus(n.id))
            .on("blur", () => focus(null)),
        )
        .attr("href", (n) => hrefOf(n.id))
        .attr("aria-label", (n) => n.title)
        .text((n) => n.title);

      simulation.nodes(nodes);
      simulation.force<ReturnType<typeof forceLink<Node, Link>>>("link")?.links(links);
      if (tracking) fit(null, false);
      repaint();
      // Whatever already has a place is held there while new entities find
      // theirs, so a link's spring can't pull a settled layout out of shape.
      // The rest let go when it ends. With nothing new there is nothing to
      // settle, and nothing moves.
      if (unsettled.size > 0) {
        for (const n of nodes) {
          if (unsettled.has(n.id)) continue;
          n.fx = n.x;
          n.fy = n.y;
        }
        settle(false);
        simulation.alpha(1).restart();
      } else {
        save();
        settle(true);
      }
    },
    fit: () => fit(null, true),
    stop() {
      simulation.stop();
      save();
      cancelAnimationFrame(frame);
      observer.disconnect();
      dark.removeEventListener("change", repaint);
      root.interrupt();
      root.on(".zoom", null).on(".fit", null).on(".focus", null).on(".open", null).on(".drag", null);
    },
  };
}

function Canvas() {
  const data = useRead((g) => g.connections());
  const view = useView();
  const router = useRouter();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const drawing = useRef<Drawing | null>(null);
  // d3 outlives renders, so it is given the latest of these when it needs them.
  const latest = useRef({ view, router });
  latest.current = { view, router };

  useEffect(() => {
    const list = listRef.current!;
    drawing.current = draw(
      canvasRef.current!,
      list,
      (id, aside) => {
        const { view, router } = latest.current;
        router.push(aside ? view.toSide(id) : view.toMain(id));
      },
      (id) => process.env.NEXT_PUBLIC_QUIDDITY_BASE_PATH + latest.current.view.toMain(id),
    );
    return () => {
      drawing.current?.stop();
      drawing.current = null;
      list.replaceChildren();
    };
  }, []);

  useEffect(() => drawing.current?.update(data), [data]);

  return (
    <div
      role="group"
      aria-label="Graph of entities"
      className="relative h-full min-h-80"
    >
      <canvas ref={canvasRef} aria-hidden className="absolute inset-0 size-full touch-none" />
      {/* The same entities as links, for keyboards and screen readers. */}
      <div ref={listRef} className="sr-only" />
      <button
        type="button"
        onClick={() => drawing.current?.fit()}
        className="absolute right-3 top-3 rounded border border-slate-300 bg-white/80 px-2 py-1 text-sm text-slate-700 hover:bg-white dark:border-slate-600 dark:bg-slate-900/80 dark:text-slate-200 dark:hover:bg-slate-900"
      >
        Reset view
      </button>
      {data.nodes.length === 0 && (
        <p className="absolute inset-x-0 top-8 text-center text-slate-500 dark:text-slate-400">
          Nothing to draw yet.
        </p>
      )}
    </div>
  );
}

/** Every entity as a point, and the links between them as lines. */
export function GraphView() {
  return (
    <Region
      pending={<Skeleton className="m-6 h-64" />}
      failed={(error) => <p className="p-6">The graph could not be drawn: {error.message}</p>}
    >
      <Canvas />
    </Region>
  );
}
