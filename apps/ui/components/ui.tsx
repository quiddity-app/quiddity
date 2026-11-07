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
  "rounded px-1.5 py-0.5 text-sm text-ink-secondary hover:bg-surface-hover hover:text-ink";

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
      className={`rounded border ${size} ${buttonVariant[variant]} disabled:text-ink-muted disabled:hover:bg-transparent ${className}`}
      {...rest}
    />
  );
}

/** A single-line text field. The caller labels it, and adds only layout. */
export function Input({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`rounded border border-control bg-transparent px-2 py-1 ${className}`}
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
