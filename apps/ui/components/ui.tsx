import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";

/*
 * The few things every screen repeats, defined once (docs/Design language.md):
 * a button, a text field, an error line, a section heading, a link and a key
 * hint. A component that wants one uses it from here instead of typing the
 * classes again, so they can't drift apart.
 */

/** A link: the link colour, underlined on hover. */
export const linkClass = "text-link underline-offset-2 hover:underline";

/** A link to somewhere that doesn't exist yet: dashed, solid on hover. */
export const unresolvedLinkClass =
  "text-link underline decoration-dashed underline-offset-2 hover:decoration-solid";

/** A section's small-capitals caption, on the page or in a card. */
export const sectionHeading = "text-xs font-medium uppercase tracking-wide text-ink-muted";

/**
 * The same caption on a sidebar's ground, where the muted colour is too faint
 * to read: it only holds up on the page itself.
 */
export const paneHeading = "text-xs font-medium uppercase tracking-wide text-ink-secondary";

/** A button of one glyph, or a few words, in a toolbar or a card's header. */
export const iconButtonClass =
  "inline-flex min-h-8 min-w-8 items-center justify-center rounded px-1.5 text-sm text-ink-secondary hover:bg-surface-hover hover:text-ink coarse:min-h-11 coarse:min-w-11";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  /**
   * `danger` is for what can't be taken back without a fuss, and is always
   * asked about first. `overlay` floats over a canvas, so it is part see-through.
   */
  variant?: "default" | "danger" | "overlay";
  /** A smaller button, for a toolbar. */
  compact?: boolean;
};

const buttonVariant = {
  default: "border-control hover:bg-surface-hover",
  danger: "border-danger text-danger hover:bg-surface-hover",
  overlay: "border-control bg-surface/80 text-ink hover:bg-surface",
};

/** A labelled action. There is one weight of button: outlined, never filled. */
export function Button({
  variant = "default",
  compact = false,
  type = "button",
  className = "",
  ...rest
}: ButtonProps) {
  const size = compact ? "px-2 py-1 text-sm" : "px-3 py-1";
  return (
    <button
      type={type}
      className={`min-h-8 rounded border coarse:min-h-11 ${size} ${buttonVariant[variant]} disabled:text-ink-muted disabled:hover:bg-transparent ${className}`}
      {...rest}
    />
  );
}

/** A single-line text field. The caller labels it, and adds only layout. */
export function Input({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`min-h-8 rounded border border-control bg-transparent px-2 py-1 coarse:min-h-11 ${className}`}
      {...rest}
    />
  );
}

/** What went wrong, in words, where it went wrong. */
export function ErrorLine({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <p role="alert" className={`text-danger ${className}`}>
      {children}
    </p>
  );
}

/** The key or chord that triggers an action. */
export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="rounded bg-surface-hover px-1 text-xs">{children}</kbd>;
}

/**
 * The row under a list that adds to it. With a mouse it stays out of the way
 * until the pointer or the keyboard reaches it, because it is wide and the
 * pointer crosses it all the time; a hover only outlines it, where a filled
 * button would flash across the page. Where there is no hover it is simply
 * there. When the list is empty it is the page's only content and is always
 * shown.
 */
export function AddRow({
  empty,
  emptyLabel,
  label,
  onAdd,
}: {
  empty: boolean;
  emptyLabel: string;
  label: string;
  onAdd: () => void;
}) {
  return (
    <button
      type="button"
      className={`mt-1 block min-h-8 w-full rounded border border-transparent px-2 py-1 text-left text-sm text-ink-muted hover:border-border motion-safe:transition-[opacity,border-color] motion-safe:duration-150 coarse:min-h-11 ${
        empty ? "" : "opacity-0 hover:opacity-100 focus:opacity-100 no-hover:opacity-100"
      }`}
      onClick={onAdd}
    >
      {empty ? emptyLabel : label}
    </button>
  );
}
