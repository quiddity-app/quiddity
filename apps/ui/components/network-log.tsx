'use client';

import { useState } from 'react';

import { Button, subsectionTitle } from './ui';

import type { SyncDetails } from '@/lib/model';

/** A log line as plain text, for copying into a bug report. */
function logLine({ at, level, device, message }: SyncDetails['log'][number]) {
  const who = device === null ? '' : ` ${device}`;
  return `${new Date(at).toISOString()} ${level}${who}: ${message}`;
}

/**
 * What the network has lately done, newest first: who was dialled, who
 * dialled, who was refused and why a connection ended. It is how to tell, say,
 * two devices that hold different graphs from two that can't reach each other.
 * It holds device IDs and reasons, and nothing from the graph.
 */
export function NetworkLog({
  details,
  showDevice = true,
}: Readonly<{ details: SyncDetails; showDevice?: boolean }>) {
  const [copied, setCopied] = useState(false);
  const { device, log } = details;
  const newest = [...log].reverse();
  return (
    <div className="mt-4">
      {showDevice ? (
        <>
          <h3 className={subsectionTitle}>This device</h3>
          <p className="mb-4 break-all">
            <code className="font-mono text-sm">{device}</code>
          </p>
        </>
      ) : null}
      <h3 className={subsectionTitle}>Network log</h3>
      {newest.length === 0 ? (
        <p className="text-ink-secondary">Nothing has happened yet.</p>
      ) : (
        <>
          <p className="mb-2">
            <Button
              compact
              onClick={() => {
                void navigator.clipboard
                  .writeText(log.map((event) => logLine(event)).join('\n'))
                  .then(() => {
                    setCopied(true);
                  })
                  .catch(() => {
                    setCopied(false);
                  });
              }}
            >
              {copied ? 'Copied' : 'Copy log'}
            </Button>
          </p>
          <ul aria-label="Network log" className="space-y-1 text-sm">
            {newest.map((event, i) => (
              <li
                key={`${event.at}-${i}`}
                data-level={event.level}
                className="grid grid-cols-[auto_1fr] gap-x-3"
              >
                <time
                  dateTime={new Date(event.at).toISOString()}
                  className="tabular-nums text-ink-secondary"
                >
                  {new Date(event.at).toLocaleTimeString()}
                </time>
                <span
                  className={event.level === 'problem' ? 'text-danger' : ''}
                >
                  {event.level === 'problem' ? 'Problem: ' : ''}
                  {event.device === null ? null : (
                    <code title={event.device} className="font-mono">
                      {event.device.length > 12
                        ? `${event.device.slice(0, 8)}…`
                        : event.device}
                    </code>
                  )}{' '}
                  {event.message}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
