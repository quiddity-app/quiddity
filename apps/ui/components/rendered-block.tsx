"use client";

import { createElement, memo, useMemo, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Block, Property, ResolvedLink } from "@/lib/model";
import { isExternal, openExternal } from "@/lib/open-external";
import { PROP_CLOSE, PROP_OPEN, remarkQuiddity } from "@/lib/remark-quiddity";
import { LinkOrNot, PropertyChip } from "./property-chip";

/**
 * A block's text split for display: line properties come out, to be drawn as
 * chips; inline ones are swapped for placeholders the Markdown renderer hands
 * back as elements; the rest is Markdown.
 */
export function splitBlock(block: Pick<Block, "text" | "properties">) {
  const inlineProps = block.properties.filter((p) => p.inline);
  const lineProps = block.properties.filter((p) => !p.inline);
  let prose = block.text;
  // From the end, so the offsets of those not yet replaced stay right.
  for (const p of [...block.properties].sort((a, b) => b.start - a.start)) {
    if (p.inline) {
      const placeholder = `${PROP_OPEN}${inlineProps.indexOf(p)}${PROP_CLOSE}`;
      prose = prose.slice(0, p.start) + placeholder + prose.slice(p.end);
    } else {
      const eol = prose[p.end] === "\n" ? 1 : 0;
      prose = prose.slice(0, p.start) + prose.slice(p.end + eol);
    }
  }
  return { prose, lineProps, inlineProps };
}

const code = "rounded bg-slate-200 px-1 py-0.5 font-mono text-[0.9em] dark:bg-slate-700";
const muted = "text-slate-500 dark:text-slate-400";

/** An element of this tag with these classes, for the renderer to use. */
const styled = (tag: string, className: string) =>
  function Styled({ children }: { children?: ReactNode }) {
    return createElement(tag, { className }, children);
  };

function components(links: Record<string, ResolvedLink>, inline: Property[]): Components {
  const map = {
    p: styled("p", "whitespace-pre-line break-words"),
    h1: styled("h1", "text-xl font-semibold"),
    h2: styled("h2", "text-lg font-semibold"),
    h3: styled("h3", "text-base font-semibold"),
    h4: styled("h4", "font-semibold"),
    h5: styled("h5", "font-semibold"),
    h6: styled("h6", "font-semibold"),
    ul: styled("ul", "list-disc pl-5"),
    ol: styled("ol", "list-decimal pl-5"),
    // A task's checkbox is its marker.
    li: ({ className, children }: { className?: string; children?: ReactNode }) => (
      <li className={className?.includes("task-list-item") ? "list-none" : undefined}>{children}</li>
    ),
    blockquote: styled(
      "blockquote",
      "border-l-2 border-slate-300 pl-3 text-slate-600 dark:border-slate-600 dark:text-slate-300",
    ),
    hr: styled("hr", "my-2 border-slate-300 dark:border-slate-600"),
    pre: styled("pre", `overflow-x-auto ${code} p-2`),
    table: styled("table", "border-collapse text-sm"),
    th: styled("th", "border border-slate-300 px-2 py-1 dark:border-slate-600"),
    td: styled("td", "border border-slate-300 px-2 py-1 dark:border-slate-600"),
    // Inline code only: inside a `pre` the block's own background is enough.
    code: ({ className, children }: { className?: string; children?: ReactNode }) => (
      <code className={className ?? code}>{children}</code>
    ),
    // A remote image would be a request to whoever wrote it.
    img: ({ alt }: { alt?: string }) => <span className={muted}>[{alt || "image"}]</span>,
    a: ({ href, children }: { href?: string; children?: ReactNode }) =>
      isExternal(href) ? (
        <a
          href={href}
          rel="noopener noreferrer"
          className="text-sky-700 underline-offset-2 hover:underline dark:text-sky-300"
          onClick={(e) => {
            e.preventDefault();
            void openExternal(href);
          }}
        >
          {children}
        </a>
      ) : (
        <span className={muted}>{children}</span>
      ),
    // Elements of our own, made by remarkQuiddity.
    "quiddity-link": ({ name }: { name?: string }) => (
      <LinkOrNot linkKey={name ?? ""} links={links} />
    ),
    "quiddity-ref": ({ id }: { id?: string }) => <span className={muted}>(({id}))</span>,
    "quiddity-prop": ({ index }: { index?: string }) => {
      const property = inline[Number(index)];
      return property ? <PropertyChip property={property} links={links} /> : null;
    },
  };
  return map as Components;
}

/** A block's text as it reads, not as it is written. */
export const RenderedBlock = memo(function RenderedBlock({
  block,
  links,
}: {
  block: Pick<Block, "text" | "properties">;
  links: Record<string, ResolvedLink>;
}): ReactNode {
  const { prose, lineProps, inlineProps } = useMemo(() => splitBlock(block), [block]);
  const map = useMemo(() => components(links, inlineProps), [links, inlineProps]);
  return (
    <>
      {prose.trim() !== "" && (
        <ReactMarkdown remarkPlugins={[remarkGfm, remarkQuiddity]} components={map}>
          {prose}
        </ReactMarkdown>
      )}
      {lineProps.length > 0 && (
        <div className={`flex flex-wrap gap-1.5 ${prose.trim() !== "" ? "mt-1" : ""}`}>
          {lineProps.map((p) => (
            <PropertyChip key={p.start} property={p} links={links} />
          ))}
        </div>
      )}
    </>
  );
});
