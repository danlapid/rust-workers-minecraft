import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { parseArgs } from 'node:util';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { play, status, waitFor, viewChunkCount } from './client.mjs';
import { testServer } from './server.mjs';

const repo = fileURLToPath(new URL('../', import.meta.url));
const { values } = parseArgs({ options: {
  seed: { type: 'string', default: '1789195700909824175' },
  view: { type: 'string', default: '4' },
  simulation: { type: 'string', default: '3' },
  players: { type: 'string', default: '2' },
  'max-players': { type: 'string' },
  compression: { type: 'string', default: '512' },
  level: { type: 'string', default: '1' },
  label: { type: 'string', default: 'experience' },
  steps: { type: 'string', default: '32' },
} });
const playerCount = Number(values.players);
const steps = Number(values.steps);
assert.ok(Number.isInteger(playerCount) && playerCount >= 1 && playerCount <= 20);
assert.ok(Number.isInteger(steps) && steps >= 0 && steps <= 128);
const vars = {
  VIEW_DISTANCE: values.view, SIMULATION_DISTANCE: values.simulation,
  MAX_PLAYERS: values['max-players'] ?? values.players, COMPRESSION_THRESHOLD: values.compression,
  COMPRESSION_LEVEL: values.level, IDLE_TIMEOUT_SECONDS: '0',
};
const { probe, startWorker, request, checkpoint, seed } = await testServer({ seed: values.seed, vars });
const worker = await startWorker();
const clients = [], samples = [];
let phase = 'login', sampling = true, sampleFailure, failure, joinedMs, starting;
const began = performance.now();
const elapsed = () => performance.now() - began;
const healthy = () => {
  worker.healthy();
  clients.forEach(c => c.assertHealthy());
  if (sampleFailure) throw sampleFailure;
};
const sampler = (async () => {
  while (sampling) {
    const beganRequest = performance.now();
    const state = await request();
    if (state.failure) throw new Error(state.failure);
    if (state.server) samples.push({ elapsed_ms: elapsed(), phase, status_ms: performance.now() - beganRequest, ...state.server });
    await delay(100);
  }
})().catch(error => { sampleFailure = error; });
const expectedChunks = viewChunkCount(Number(values.view));
const destinations = [];
let pingMs;
try {
  const pingStart = performance.now();
  await status();
  pingMs = performance.now() - pingStart;
  assert.equal((await request()).runtime_starts, 0);
  for (let i = 0; i < playerCount; i++) clients.push(await play(`Probe${String.fromCharCode(65 + i)}`));
  await waitFor(() => { healthy(); return clients.every(c => c.chunks.size >= expectedChunks); }, 'complete initial views');
  joinedMs = elapsed();
  starting = await request();
  phase = 'stationary';
  await delay(5000);
  healthy();
  const origins = clients.map(c => ({ ...c.position }));
  const directions = playerCount <= 4 ? [[1, 0], [-1, 0], [0, 1], [0, -1]]
    : Array.from({ length: playerCount }, (_, i) => [Math.cos(i * 2 * Math.PI / playerCount), Math.sin(i * 2 * Math.PI / playerCount)]);
  phase = 'exploration';
  for (let step = 1; step <= steps; step++) {
    for (let i = 0; i < clients.length; i++) {
      destinations[i] = { ...origins[i], x: origins[i].x + directions[i][0] * step * 4, z: origins[i].z + directions[i][1] * step * 4 };
      clients[i].move(destinations[i]);
    }
    await delay(500);
    healthy();
  }
  phase = 'settling';
  await delay(5000);
  healthy();
} catch (error) { failure = String(error); }
finally {
  sampling = false;
  await sampler;
  failure ??= sampleFailure && String(sampleFailure);
  const percentile = (numbers, q) => {
    if (!numbers.length) return null;
    const ordered = [...numbers].sort((a, b) => a - b);
    return ordered[Math.ceil(q * ordered.length) - 1];
  };
  const phases = {};
  for (const phase of new Set(samples.map(s => s.phase))) {
    const selected = samples.filter(s => s.phase === phase);
    const first = selected[0], last = selected.at(-1);
    phases[phase] = {
      samples: selected.length,
      status_p95_ms: percentile(selected.map(s => s.status_ms), 0.95),
      status_max_ms: Math.max(...selected.map(s => s.status_ms)),
      observed_tps: selected.length > 1 ? (last.ticks - first.ticks) * 1000 / (last.elapsed_ms - first.elapsed_ms) : null,
      mean_tick_ms: last.mean_tick_ms,
    };
  }
  const result = {
    label: values.label, seed, vars, players: playerCount, steps,
    wasm_sha256: createHash('sha256').update(await readFile(path.join(repo, 'target/workers/wasm32-unknown-emscripten/release/pumpkin_do.wasm'))).digest('hex'),
    status_ping_ms: pingMs, startup_ms: starting?.startup_ms, joined_ms: joinedMs,
    peak_wasm_bytes: samples.length ? Math.max(...samples.map(s => s.wasm_memory_bytes)) : null,
    peak_heap_bytes: samples.length ? Math.max(...samples.map(s => s.heap_allocated_bytes)) : null,
    final_sample: samples.at(-1), phases,
    clients: clients.map((c, i) => ({ chunks_received: c.chunks.size, ...c.traffic, commanded_destination: destinations[i] })),
    failure: failure ?? null, samples,
  };
  await writeFile(path.join(probe, 'experience.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ ...result, samples: `${samples.length} samples in ${probe}/experience.json` }, null, 2));
  await Promise.allSettled(clients.map(c => c.close()));
  try { if (!failure) await checkpoint(worker); }
  finally { await worker.stop(); }
}
if (failure) throw new Error(failure);
console.log('PUMPKIN-EXPERIENCE-PROBE-OK');
