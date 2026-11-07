"use client";

import { Component, Suspense, type ReactNode } from "react";

class ErrorBoundary extends Component<
  { fallback: (error: Error) => ReactNode; children: ReactNode },
  { error: Error | null }
> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    return this.state.error ? this.props.fallback(this.state.error) : this.props.children;
  }
}

/**
 * Every independently loading region gets one of these, so that a slow or
 * failing region neither blocks nor takes down its neighbours.
 */
export function Region({
  pending,
  failed,
  children,
}: {
  pending: ReactNode;
  failed: (error: Error) => ReactNode;
  children: ReactNode;
}) {
  return (
    <ErrorBoundary fallback={failed}>
      <Suspense fallback={pending}>{children}</Suspense>
    </ErrorBoundary>
  );
}

export function Skeleton({ className = "" }: { className?: string }) {
  return (
    <div
      aria-hidden
      className={`motion-safe:animate-pulse rounded bg-surface-hover ${className}`}
    />
  );
}
