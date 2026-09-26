import assert from 'node:assert/strict';
import path from 'node:path';
import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { play, waitFor, viewChunkCount } from './client.mjs';
import { testServer } from './server.mjs';

const repo = fileURLToPath(new URL('../', import.meta.url));
const { values } = parseArgs({ options: { seed: { type: 'string' } } });
const { probe, startWorker, request, checkpoint, seed } = await testServer(values);
console.log(`Memory probe seed: ${seed}`);

const worker = await startWorker();
const clients = [];
const samples = [];
let phase = 'login', sampling = true, sampleFailure;
const started = Date.now();
const sampler = (async () => {
  while (sampling) {
    worker.healthy();
    const state = await request();
    if (state.failure) throw new Error(state.failure);
    if (state.server) {
      const memory = state.server;
      assert.ok(memory.heap_allocated_bytes > 0);
      assert.equal(memory.heap_allocated_bytes + memory.heap_free_bytes, memory.heap_arena_bytes);
      assert.ok(memory.heap_arena_bytes < memory.wasm_memory_bytes);
      samples.push({ elapsed_ms: Date.now() - started, phase, ...memory });
    }
    await delay(100);
  }
})().catch(error => { sampleFailure = error; });
try {
  const expectedChunks = viewChunkCount((await request()).settings.viewDistance);
  clients.push(await play('ProbeA'));
  clients.push(await play('ProbeB'));
  await waitFor(() => {
    worker.healthy();
    clients.forEach(client => client.assertHealthy());
    if (sampleFailure) throw sampleFailure;
    return clients.every(client => client.chunks.size >= expectedChunks);
  }, 'both clients to receive their view distance');
  const joinedMs = Date.now() - started;
  phase = 'stationary';
  await delay(30_000);
  clients.forEach(client => client.assertHealthy());
  sampling = false;
  await sampler;
  if (sampleFailure) throw sampleFailure;
  worker.healthy();
  const settled = await request();
  // Stop polling before disconnecting: workerd can cancel a concurrent status
  // RPC while the TCP event closes. checkpoint() waits for storage separately.
  for (const client of clients) await client.close();
  clients.length = 0;
  await checkpoint(worker);
  assert.ok(samples.length > 0);
  const result = {
    commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(),
    wasm_sha256: createHash('sha256').update(await readFile(path.join(repo,
      'target/workers/wasm32-unknown-emscripten/release/pumpkin_do.wasm'))).digest('hex'),
    glue_sha256: createHash('sha256').update(await readFile(path.join(repo,
      'target/workers/wasm32-unknown-emscripten/release/pumpkin-do.js'))).digest('hex'),
    seed, joined_ms: joinedMs, settled: settled.server,
    peak_wasm_bytes: Math.max(...samples.map(sample => sample.wasm_memory_bytes)),
    peak_sampled_heap_bytes: Math.max(...samples.map(sample => sample.heap_allocated_bytes)),
    samples,
  };
  await writeFile(path.join(probe, 'memory.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ ...result, samples: `${samples.length} samples in ${probe}/memory.json` }, null, 2));
  console.log('PUMPKIN-MEMORY-PROBE-OK');
} finally {
  sampling = false;
  await sampler;
  await Promise.all(clients.map(client => client.close()));
  if (clients.length) await checkpoint(worker).catch(error => console.error(error.message));
  await worker.stop();
}
