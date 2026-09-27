import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { setImmediate as settle } from 'node:timers/promises';
import { runInNewContext } from 'node:vm';
import test from 'node:test';

const source = await readFile(new URL('../src/workerd.js', import.meta.url), 'utf8');

function hostTasks() {
  let library;
  const waits = [], errors = [];
  const host = {
    AbortController,
    addToLibrary(value) { library = value; },
    queueMicrotask(callback) { errors.push(callback); },
    scheduler: {
      wait(delay, { signal }) {
        assert.equal(delay, 0);
        return new Promise((resolve, reject) => {
          signal.addEventListener('abort', () => reject(signal.reason), { once: true });
          waits.push({ signal, resolve, reject });
        });
      },
    },
  };
  runInNewContext(source, host);
  library.$workerdHostTasks();
  return { host, waits, errors };
}

test('a host task runs asynchronously once and releases its handle', async () => {
  const { host, waits } = hostTasks();
  let calls = 0;
  const id = host.emSetImmediate(() => calls++);
  assert.ok(Number.isInteger(id) && id > 0);
  assert.equal(calls, 0);
  waits[0].resolve();
  await settle();
  assert.equal(calls, 1);
  assert.equal(host.emClearImmediate(id), false);
});

test('cancelling one host task aborts its wait without cancelling another', async () => {
  const { host, waits, errors } = hostTasks();
  const calls = [];
  const cancelled = host.emSetImmediate(() => calls.push('cancelled'));
  host.emSetImmediate(() => calls.push('live'));
  assert.equal(host.emClearImmediate(cancelled), true);
  assert.equal(host.emClearImmediate(cancelled), false);
  assert.equal(waits[0].signal.aborted, true);
  assert.equal(waits[1].signal.aborted, false);
  waits[1].resolve();
  await settle();
  assert.deepEqual(calls, ['live']);
  assert.equal(errors.length, 0);
});

test('cancellation wins after the wait resolves but before the callback runs', async () => {
  const { host, waits, errors } = hostTasks();
  let called = false;
  const id = host.emSetImmediate(() => { called = true; });
  waits[0].resolve();
  assert.equal(host.emClearImmediate(id), true);
  await settle();
  assert.equal(called, false);
  assert.equal(errors.length, 0);
});

test('host task errors surface instead of becoming ignored promise rejections', async () => {
  const { host, waits, errors } = hostTasks();
  const id = host.emSetImmediate(() => { throw new Error('callback failed'); });
  waits[0].resolve();
  await settle();
  assert.equal(host.emClearImmediate(id), false);
  assert.equal(errors.length, 1);
  assert.throws(errors[0], /callback failed/);
});
