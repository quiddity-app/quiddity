'use client';

import { useEffect, useState } from 'react';

import { Button } from './ui';

import { checkForUpdate, installUpdate, useUpdates } from '@/lib/updates';

/**
 * Offers the update the app has found. It looks once when the app opens, and
 * again when the user asks (the More menu, or the app menu on macOS), in which
 * case it also says when there is nothing newer or the check failed. Nothing
 * is installed until the user asks; installing restarts the app, so it says
 * so. It draws nothing in a browser, or while there is nothing to say.
 */
export function UpdateNotice() {
  const { update, status } = useUpdates();
  const [state, setState] = useState<'idle' | 'installing' | 'failed'>('idle');

  useEffect(() => {
    void checkForUpdate();
  }, []);

  if (update === null) {
    // Nothing is drawn while idle, so the page has no status region of its
    // own to be mistaken for another's (the tests, and Stats, look for one).
    if (status === 'idle') {
      return null;
    }
    const words = {
      checking: 'Checking for updates…',
      current: 'Up to date',
      failed: "Couldn't check for updates",
    }[status];
    return (
      <span role="status" className="text-sm text-ink-secondary">
        {words}
      </span>
    );
  }
  const install = () => {
    setState('installing');
    installUpdate().catch((error) => {
      console.error("Couldn't install the update", error);
      setState('failed');
    });
  };
  let label = `Update to ${update.version}`;
  if (state === 'installing') {
    label = 'Updating…';
  } else if (state === 'failed') {
    label = 'Update failed: retry';
  }
  return (
    <Button
      compact
      onClick={install}
      disabled={state === 'installing'}
      title={update.notes ?? undefined}
      aria-label={
        state === 'failed'
          ? `Updating to ${update.version} failed. Try again`
          : `Update to ${update.version} and restart`
      }
    >
      {label}
    </Button>
  );
}
