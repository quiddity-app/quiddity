"use client";

import { useEffect, useState } from "react";
import type { Asset, EntityId } from "@/lib/model";
import { useGraph, useRead } from "@/lib/use-graph";
import { Button, linkClass } from "./ui";

type Loaded = { key: string } & ({ url: string } | { reason: string });

/**
 * An object URL for a file's bytes, made once they are here and let go when
 * the page moves on. `attempt` asks again: the bytes may have arrived from
 * another device since. Undefined while it is being read.
 */
function useFileUrl(hash: string, mime: string, attempt: number): Loaded | undefined {
  const graph = useGraph();
  const key = `${hash}/${mime}/${attempt}`;
  const [loaded, setLoaded] = useState<Loaded>();
  useEffect(() => {
    let cancelled = false;
    let url: string | undefined;
    graph.readFile(hash).then(
      (bytes) => {
        if (cancelled) return;
        url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
        setLoaded({ key, url });
      },
      (error: Error) => {
        if (!cancelled) setLoaded({ key, reason: error.message });
      },
    );
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [graph, hash, mime, key]);
  return loaded?.key === key ? loaded : undefined;
}

const units = ["bytes", "KB", "MB", "GB", "TB"];

/** A size for people: 1.5 MB, not 1572864. */
export function formatSize(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  const shown = unit === 0 || value >= 100 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${shown} ${units[unit]}`;
}

function File({ asset }: { asset: Asset }) {
  const graph = useGraph();
  const [attempt, setAttempt] = useState(0);
  const file = useFileUrl(asset.hash, asset.mime, attempt);
  const name = asset.name ?? asset.hash.slice(0, 12);
  const details = `${formatSize(asset.size)}, ${asset.mime}`;

  if (!graph.canKeepFiles()) {
    return <p role="status">This build can’t show files. {name} is {details}.</p>;
  }
  if (!file) return <p role="status">Loading {name}…</p>;
  if ("reason" in file) {
    return (
      <div role="status" className="space-y-2">
        <p>
          {name} ({details}) isn’t on this device yet. It arrives when a device that has it is
          reachable.
        </p>
        <Button onClick={() => setAttempt((n) => n + 1)}>Try again</Button>
      </div>
    );
  }
  return (
    <div className="space-y-2">
      {asset.mime.startsWith("image/") && (
        // A blob URL: next/image has nothing to optimise.
        // eslint-disable-next-line @next/next/no-img-element
        <img src={file.url} alt={name} className="max-h-[32rem] max-w-full rounded" />
      )}
      <p>
        <a href={file.url} download={name} className={linkClass}>
          Download {name}
        </a>{" "}
        <span className="text-ink-muted">({details})</span>
      </p>
    </div>
  );
}

/** What an asset's entity is a file of, drawn from its bytes. Nothing for any
 * other entity. */
export function AssetFile({ id }: { id: EntityId }) {
  const asset = useRead((g) => g.entity(id)).asset;
  return asset ? (
    <section aria-label="File" className="mb-4">
      <File asset={asset} />
    </section>
  ) : null;
}
