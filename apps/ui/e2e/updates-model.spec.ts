import { expect, test } from '@playwright/test';

import { parseMenuCommand } from '../lib/menu';
import { NOTE_MS, Updates, type Invoke } from '../lib/updates';

const found = { version: '1.2.3', notes: null };

/** A shell that answers each command from `answers`, and records what it was asked. */
function shell(answers: Record<string, unknown>) {
  const asked: string[] = [];
  const invoke: Invoke = (command) => {
    asked.push(command);
    const answer = answers[command];
    return answer instanceof Error
      ? Promise.reject(answer)
      : Promise.resolve(answer as never);
  };
  return { asked, invoke };
}

test('a manual check that finds nothing says so, then goes quiet', async () => {
  const { invoke } = shell({
    updates_supported: true,
    check_for_update: null,
  });
  const updates = new Updates(invoke, () => true);
  await updates.check(true);
  expect(updates.get()).toEqual({
    supported: true,
    update: null,
    status: 'current',
  });
  await new Promise((done) => setTimeout(done, NOTE_MS + 100));
  expect(updates.get().status).toBe('idle');
});

test('the check made when the app opens is quiet about nothing and failure', async () => {
  const quiet = new Updates(
    shell({ updates_supported: true, check_for_update: null }).invoke,
    () => true,
  );
  await quiet.check(false);
  expect(quiet.get().status).toBe('idle');

  const failing = new Updates(
    shell({ updates_supported: true, check_for_update: new Error('offline') })
      .invoke,
    () => true,
  );
  await failing.check(false);
  expect(failing.get().status).toBe('idle');
});

test('a manual check that fails says so', async () => {
  const updates = new Updates(
    shell({ updates_supported: true, check_for_update: new Error('offline') })
      .invoke,
    () => true,
  );
  await updates.check(true);
  expect(updates.get().status).toBe('failed');
});

test('an update that is found is kept, and a later check that finds none drops it', async () => {
  const answers: Record<string, unknown> = {
    updates_supported: true,
    check_for_update: found,
  };
  const updates = new Updates(shell(answers).invoke, () => true);
  await updates.check(true);
  expect(updates.get()).toMatchObject({ update: found, status: 'idle' });
  answers.check_for_update = null;
  await updates.check(true);
  expect(updates.get().update).toBeNull();
});

test('a check under way is joined, not repeated', async () => {
  const { asked, invoke } = shell({
    updates_supported: true,
    check_for_update: null,
  });
  const updates = new Updates(invoke, () => true);
  await Promise.all([updates.check(true), updates.check(true)]);
  expect(asked.filter((c) => c === 'check_for_update')).toHaveLength(1);
});

test('where the app cannot update, nothing is offered', async () => {
  const phone = shell({ updates_supported: false });
  const updates = new Updates(phone.invoke, () => true);
  await updates.check(true);
  expect(updates.get()).toEqual({
    supported: false,
    update: null,
    status: 'idle',
  });
  expect(phone.asked).toEqual(['updates_supported']);

  const browser = shell({});
  await new Updates(browser.invoke, () => false).check(true);
  expect(browser.asked).toEqual([]);
});

test('the menu can ask for the About page', () => {
  expect(parseMenuCommand('about')).toEqual({ kind: 'go', path: '/about/' });
  expect(parseMenuCommand('go:/about/')).toBeNull();
});

test('the menu can ask for a check', () => {
  expect(parseMenuCommand('check-for-updates')).toEqual({
    kind: 'check-for-updates',
  });
  expect(parseMenuCommand('check-for-update')).toBeNull();
});
