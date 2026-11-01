"use client";

import { use, useState } from "react";
import { useGraph, useRead } from "@/lib/use-graph";
import { Region, Skeleton } from "./boundary";

const when = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

const button =
  "rounded border border-slate-300 px-3 py-1 hover:bg-slate-200 dark:border-slate-600 dark:hover:bg-slate-700";

/** An ID, drawn to be read and copied: long, and no use cut short. */
function Id({ id }: { id: string }) {
  return <code className="break-all font-mono text-sm">{id}</code>;
}

function ThisDevice() {
  const graph = useGraph();
  const id = use(graph.deviceId());
  const [copied, setCopied] = useState(false);
  return (
    <section aria-labelledby="this-device" className="mb-8">
      <h2 id="this-device" className="mb-2 text-lg font-semibold">
        This device
      </h2>
      <p className="mb-2">
        Another device needs this ID to let this one in. Add the other device’s ID below too, so each lets the
        other in.
      </p>
      <p className="mb-2 flex flex-wrap items-center gap-3">
        <Id id={id} />
        <button
          type="button"
          className={button}
          onClick={() => {
            void navigator.clipboard
              .writeText(id)
              .then(() => setCopied(true))
              .catch(() => setCopied(false));
          }}
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </p>
    </section>
  );
}

function Listed() {
  const devices = useRead((g) => g.devices());
  const graph = useGraph();
  const mine = graph.canPair() ? use(graph.deviceId()) : undefined;
  return (
    <section aria-labelledby="in-graph" className="mb-8">
      <h2 id="in-graph" className="mb-2 text-lg font-semibold">
        Devices in this graph
      </h2>
      {devices.length === 0 ? (
        <p>None yet. A device is listed once it has saved the graph, or been let in.</p>
      ) : (
        <ul className="space-y-2">
          {devices.map((device) => {
            const date = new Date(device.seen);
            return (
              <li key={device.id} className="flex flex-wrap items-baseline gap-x-3">
                <Id id={device.id} />
                {device.id === mine ? <span className="font-medium">this device</span> : null}
                <span className="text-slate-500 dark:text-slate-400">
                  last saved <time dateTime={date.toISOString()}>{when.format(date)}</time>
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

type Outcome = { kind: "added"; id: string } | { kind: "failed"; message: string } | null;

function Add() {
  const graph = useGraph();
  const [id, setId] = useState("");
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<Outcome>(null);
  return (
    <section aria-labelledby="add-device" className="mb-8">
      <h2 id="add-device" className="mb-2 text-lg font-semibold">
        Add a device
      </h2>
      <p className="mb-2">
        On the other device, open this page and copy its ID. Paste it here. Then do the same there with this
        device’s ID.
      </p>
      <form
        className="flex flex-wrap items-center gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          const device = id.trim();
          if (device === "") return;
          setBusy(true);
          graph
            .allowDevice(device)
            .then(() => {
              setOutcome({ kind: "added", id: device });
              setId("");
            })
            .catch((error: Error) => setOutcome({ kind: "failed", message: error.message }))
            .finally(() => setBusy(false));
        }}
      >
        <label className="flex flex-1 flex-wrap items-center gap-3">
          <span>Device ID</span>
          <input
            type="text"
            value={id}
            onChange={(event) => setId(event.target.value)}
            spellCheck={false}
            autoComplete="off"
            className="min-w-64 flex-1 rounded border border-slate-300 bg-transparent px-2 py-1 font-mono text-sm dark:border-slate-600"
          />
        </label>
        <button type="submit" className={button} disabled={busy || id.trim() === ""}>
          Add device
        </button>
      </form>
      <div aria-live="polite" className="mt-2">
        {outcome?.kind === "added" ? (
          <p role="status">
            Added. It will connect when both devices are online and have each other’s ID.
          </p>
        ) : null}
        {outcome?.kind === "failed" ? (
          <p role="alert" className="text-red-700 dark:text-red-300">
            {outcome.message}
          </p>
        ) : null}
      </div>
    </section>
  );
}

function Pairing() {
  const graph = useGraph();
  if (!graph.canPair()) {
    return <p className="mb-8">Sharing this graph with other devices is only available in the app.</p>;
  }
  return (
    <>
      <ThisDevice />
      <Add />
    </>
  );
}

export function Devices() {
  return (
    <article className="mx-auto max-w-4xl px-6 py-8">
      <h1 className="mb-4 text-2xl font-semibold">Devices</h1>
      <Region
        pending={<Skeleton className="h-4 w-1/2" />}
        failed={(error) => (
          <p role="alert" className="text-red-700 dark:text-red-300">
            {error.message}
          </p>
        )}
      >
        <Pairing />
        <Listed />
      </Region>
    </article>
  );
}
