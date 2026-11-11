"use client";

import { useEffect, useState } from "react";
import { canUpdate, checkForUpdate, installUpdate, type Update } from "@/lib/updates";
import { Button } from "./ui";

/**
 * Offers the update the app has found, once, when it opens. Nothing is
 * installed until the user asks; installing restarts the app, so it says so.
 * It draws nothing in a browser, or while there is nothing newer.
 */
export function UpdateNotice() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [state, setState] = useState<"idle" | "installing" | "failed">("idle");

  useEffect(() => {
    if (!canUpdate()) return;
    let current = true;
    void checkForUpdate().then((found) => current && setUpdate(found));
    return () => {
      current = false;
    };
  }, []);

  if (update === null) return null;
  const install = () => {
    setState("installing");
    installUpdate().catch((error) => {
      console.error("Couldn't install the update", error);
      setState("failed");
    });
  };
  return (
    <Button
      compact
      onClick={install}
      disabled={state === "installing"}
      title={update.notes ?? undefined}
      aria-label={
        state === "failed"
          ? `Updating to ${update.version} failed. Try again`
          : `Update to ${update.version} and restart`
      }
    >
      {state === "installing"
        ? "Updating…"
        : state === "failed"
          ? "Update failed: retry"
          : `Update to ${update.version}`}
    </Button>
  );
}
