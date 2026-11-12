"use client";

import { useEffect, useRef, useState } from "react";
import { chooseBackend, type Backend } from "@/lib/graph";
import type { Problem, Recovery as How } from "@/lib/recovery";
import { Button, ErrorLine, pageTitle, sectionTitle } from "./ui";

/**
 * Stands in for the graph when the app couldn't open the one on this device,
 * rather than the app exiting with no window to say why. It covers the page:
 * nothing behind it can work. Nothing it offers deletes anything: starting
 * again sets the damaged files aside first.
 *
 * It draws nothing when the graph opened, or outside the app.
 */
export function Recovery() {
  const [problem, setProblem] = useState<Problem | null>(null);
  const [backend, setBackend] = useState<Backend | null>(null);
  const [working, setWorking] = useState<How | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let current = true;
    void (async () => {
      try {
        const chosen = await chooseBackend();
        const found = (await chosen.startupProblem?.()) ?? null;
        if (current && found) {
          setBackend(chosen);
          setProblem(found);
        }
      } catch (error) {
        // Not being able to ask is not a reason to cover the page.
        console.info("Couldn't ask whether the graph opened", error);
      }
    })();
    return () => {
      current = false;
    };
  }, []);

  useEffect(() => {
    // The safest choice is first, so it is the one a stray Enter takes.
    if (problem) dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, [problem]);

  if (!problem || !backend) return null;
  const go = (how: How) => {
    setWorking(how);
    setFailure(null);
    // What the page already tried to load is gone with it: start the page over.
    backend
      .recover?.(how)
      .then(() => window.location.reload())
      .catch((error) => {
        setFailure(String(error instanceof Error ? error.message : error));
        setWorking(null);
      });
  };
  return (
    <div
      ref={dialog}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="recovery-title"
      aria-describedby="recovery-what"
      className="fixed inset-0 z-50 overflow-auto bg-surface p-8 text-ink"
    >
      <div className="mx-auto max-w-2xl">
        <h1 id="recovery-title" className={pageTitle}>
          Quiddity can’t open this device’s graph
        </h1>
        <div id="recovery-what" className="mb-6 space-y-2">
          <p>
            The graph in <code className="break-all font-mono text-sm">{problem.dir}</code> couldn’t be read:
          </p>
          <p className="font-mono text-sm break-words">{problem.message}</p>
          <p>Nothing has been deleted, and nothing below deletes it.</p>
        </div>
        {failure ? <ErrorLine className="mb-4">{failure}</ErrorLine> : null}
        <section aria-labelledby="recovery-retry" className="mb-6">
          <h2 id="recovery-retry" className={sectionTitle}>
            Look again
          </h2>
          <p className="mb-2">If you have put the file right yourself, or moved a backup into place.</p>
          <Button onClick={() => go("retry")} disabled={working !== null}>
            {working === "retry" ? "Looking…" : "Try again"}
          </Button>
        </section>
        <section aria-labelledby="recovery-blank" className="mb-6">
          <h2 id="recovery-blank" className={sectionTitle}>
            Get the graph back from another device
          </h2>
          <p className="mb-2">
            Sets the damaged files aside and starts empty. Let this device in from another one that has the
            graph (the Devices page shows this device’s ID), and the graph syncs back.
          </p>
          <Button onClick={() => go("blank")} disabled={working !== null}>
            {working === "blank" ? "Starting…" : "Start empty and sync"}
          </Button>
        </section>
        <section aria-labelledby="recovery-fresh" className="mb-6">
          <h2 id="recovery-fresh" className={sectionTitle}>
            Start a new graph
          </h2>
          <p className="mb-2">
            Sets the damaged files aside and starts a graph of its own. It is not the same graph as your other
            devices’, and won’t merge with theirs.
          </p>
          <Button variant="danger" onClick={() => go("fresh")} disabled={working !== null}>
            {working === "fresh" ? "Starting…" : "Start a new graph"}
          </Button>
        </section>
      </div>
    </div>
  );
}
