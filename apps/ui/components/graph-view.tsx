"use client";

import { drag } from "d3-drag";
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
import { select, type Selection } from "d3-selection";
import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
import { useRouter } from "next/navigation";
import { useEffect, useRef } from "react";
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

/** How the graph draws itself, kept apart from React: d3 owns the SVG inside. */
interface Drawing {
  update(data: Connections): void;
  stop(): void;
}

function draw(
  svg: SVGSVGElement,
  open: (id: string, aside: boolean) => void,
  hrefOf: (id: string) => string,
): Drawing {
  const root = select(svg);
  const layer = root.append("g");
  const linkLayer = layer.append("g").attr("class", "stroke-slate-300 dark:stroke-slate-600");
  const nodeLayer = layer.append("g");

  let nodes: Node[] = [];
  let links: Link[] = [];
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
  simulation.on("end", () => {
    unsettled.clear();
    release();
    save();
  });

  let neighbours = new Map<string, Set<string>>();
  let nodeSelection: Selection<SVGAElement, Node, SVGGElement, unknown> | undefined;
  let linkSelection: Selection<SVGLineElement, Link, SVGGElement, unknown> | undefined;
  let focused: string | null = null;
  // Set before the first nodes exist: the initial zoom calls this.
  const labels = () => {
    nodeSelection
      ?.select<SVGTextElement>("text")
      .attr("display", (n) =>
        n.id === focused || radius(n) * view.k >= LABEL_RADIUS ? null : "none",
      );
  };

  const behaviour: ZoomBehavior<SVGSVGElement, unknown> = zoom<SVGSVGElement, unknown>()
    .scaleExtent([0.1, 4])
    .on("zoom", (event: { transform: ZoomTransform }) => {
      view = event.transform;
      layer.attr("transform", view.toString());
      labels();
    })
    .on("end", save);
  root.call(behaviour);
  root.on("dblclick.zoom", null);
  const { width, height } = svg.getBoundingClientRect();
  const start = cachedTransform();
  root.call(
    behaviour.transform,
    start
      ? zoomIdentity.translate(start.x, start.y).scale(start.k)
      : zoomIdentity.translate(width / 2, height / 2),
  );

  const focus = (id: string | null) => {
    focused = id;
    labels();
    const near = id === null ? null : (neighbours.get(id) ?? new Set<string>());
    nodeSelection?.classed(
      "opacity-25",
      (n) => near !== null && n.id !== id && !near.has(n.id),
    );
    linkSelection?.classed(
      "opacity-10",
      (l) => id !== null && (l.source as Node).id !== id && (l.target as Node).id !== id,
    );
  };

  const place = () => {
    linkSelection
      ?.attr("x1", (l) => (l.source as Node).x ?? 0)
      .attr("y1", (l) => (l.source as Node).y ?? 0)
      .attr("x2", (l) => (l.target as Node).x ?? 0)
      .attr("y2", (l) => (l.target as Node).y ?? 0);
    nodeSelection?.attr("transform", (n) => `translate(${n.x ?? 0},${n.y ?? 0})`);
  };
  simulation.on("tick", place);

  const dragging = drag<SVGAElement, Node>()
    .on("start", (event, n) => {
      if (!event.active) simulation.alphaTarget(0.2).restart();
      // Let everything go, so the rest can untangle around the dragged node.
      release();
      n.fx = n.x;
      n.fy = n.y;
    })
    .on("drag", (event, n) => {
      n.fx = event.x;
      n.fy = event.y;
    })
    .on("end", (event, n) => {
      if (!event.active) simulation.alphaTarget(0);
      n.fx = null;
      n.fy = null;
    });

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
      let fresh = 0;
      nodes = data.nodes.map(({ id, title }) => {
        const node: Node = known.get(id) ?? { id, title, degree: 0 };
        node.title = title;
        node.degree = degree.get(id) ?? 0;
        if (node.x === undefined || node.y === undefined) {
          const remembered = cachedPosition(id);
          if (remembered) {
            node.x = remembered.x;
            node.y = remembered.y;
          } else {
            fresh++;
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
      links = data.edges.map(({ source, target }) => ({ source, target }));

      linkSelection = linkLayer
        .selectAll<SVGLineElement, Link>("line")
        .data(links)
        .join("line")
        .attr("stroke-width", 1);
      nodeSelection = nodeLayer
        .selectAll<SVGAElement, Node>("a")
        .data(nodes, (n) => n.id)
        .join((enter) => {
          const a = enter
            .append<SVGAElement>("a")
            .attr("class", "cursor-pointer outline-none [&:focus-visible_circle]:stroke-2")
            .on("click", (event: MouseEvent, n) => {
              event.preventDefault();
              open(n.id, event.shiftKey);
            })
            .on("mouseenter focus", (_event, n) => focus(n.id))
            .on("mouseleave blur", () => focus(null))
            .call(dragging);
          a.append("circle").attr(
            "class",
            "fill-sky-600 stroke-sky-900 dark:fill-sky-400 dark:stroke-sky-100",
          );
          a.append("text")
            .attr("dy", "0.32em")
            .attr("class", "fill-slate-800 dark:fill-slate-200 text-[10px] select-none");
          return a;
        })
        .attr("href", (n) => hrefOf(n.id))
        .attr("aria-label", (n) => n.title);
      nodeSelection.select<SVGCircleElement>("circle").attr("r", radius);
      labels();
      nodeSelection
        .select<SVGTextElement>("text")
        .attr("x", (n) => radius(n) + 3)
        .text((n) => n.title);

      simulation.nodes(nodes);
      simulation.force<ReturnType<typeof forceLink<Node, Link>>>("link")?.links(links);
      place();
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
        simulation.alpha(1).restart();
      } else {
        // A simulation starts running as it's made, at full heat: stop it, or
        // it jostles the layout we've just restored.
        simulation.stop();
        save();
      }
    },
    stop() {
      simulation.stop();
      save();
      root.on(".zoom", null);
    },
  };
}

function Canvas() {
  const data = useRead((g) => g.connections());
  const view = useView();
  const router = useRouter();
  const svgRef = useRef<SVGSVGElement>(null);
  const drawing = useRef<Drawing | null>(null);
  // d3 outlives renders, so it is given the latest of these when it needs them.
  const latest = useRef({ view, router });
  latest.current = { view, router };

  useEffect(() => {
    const svg = svgRef.current!;
    drawing.current = draw(
      svg,
      (id, aside) => {
        const { view, router } = latest.current;
        router.push(aside ? view.toSide(id) : view.toMain(id));
      },
      (id) => process.env.NEXT_PUBLIC_QUIDDITY_BASE_PATH + latest.current.view.toMain(id),
    );
    return () => {
      drawing.current?.stop();
      drawing.current = null;
      svg.replaceChildren();
    };
  }, []);

  useEffect(() => drawing.current?.update(data), [data]);

  return (
    <div className="relative h-full min-h-80">
      <svg
        ref={svgRef}
        role="group"
        aria-label="Graph of entities"
        className="absolute inset-0 size-full touch-none"
      />
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
