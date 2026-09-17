import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { status, play, waitFor } from './client.mjs';
import { testServer } from './server.mjs';

const { startWorker, request, checkpoint } = await testServer();

const { blocks } = JSON.parse(await readFile(new URL('../.work/pumpkin/assets/blocks.json', import.meta.url)));
const states = new Map(blocks.flatMap(block => block.states.map(state => [state.id, { ...state, name: block.name }])));

function editableBlock(client) {
  const origin = Object.fromEntries(Object.entries(client.position).map(([axis, value]) => [axis, Math.floor(value)]));
  const stateAt = position => states.get(client.block(position));
  for (const dy of [-1, -2, -3]) for (const dx of [0, -1, 1, -2, 2]) for (const dz of [0, -1, 1, -2, 2]) {
    const position = { x: origin.x + dx, y: origin.y + dy, z: origin.z + dz };
    const state = stateAt(position);
    // Pumpkin's IS_FULL_CUBE flag excludes fluids and plants. Ice becomes water when broken.
    if (Math.hypot(dx, dy + 0.5 - 1.62, dz) > 4.5 || !(state?.state_flags & (1 << 7))
      || state.hardness < 0 || state.name.includes('ice')) continue;
    const above = stateAt({ ...position, y: position.y + 1 })?.name;
    if (['sand', 'red_sand', 'gravel'].includes(above)) continue;
    // Avoid water flowing into the hole, or unsupported terrain falling into it.
    if ([[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]].every(([x, y, z]) => {
      const state = stateAt({ x: position.x + x, y: position.y + y, z: position.z + z });
      return state && !(state.state_flags & (1 << 5)); // IS_LIQUID
    })) return position;
  }
  throw new Error(`No stable block within reach of ${JSON.stringify(client.position)}`);
}

let savedChunk, savedPosition, changedBlock;
let worker = await startWorker();
const clients = [];
try {
  assert.equal((await status()).version.protocol, 776);
  const a = await play('ProbeA'); clients.push(a);
  const b = await play('ProbeB'); clients.push(b);
  await waitFor(() => {
    a.assertHealthy(); b.assertHealthy(); worker.healthy();
    return a.seenNames.has('ProbeB') && b.seenNames.has('ProbeA');
  }, 'both players to see each other');
  assert.equal((await status()).players.online, 2);
  changedBlock = editableBlock(a);
  savedChunk = `${Math.floor(changedBlock.x / 16)},${Math.floor(changedBlock.z / 16)}`;
  assert.ok(a.chunks.has(savedChunk) && b.chunks.has(savedChunk), 'Clients did not receive the spawn chunk');
  assert.notEqual(a.block(changedBlock), 0, 'Expected a solid block below the player');
  await a.breakBlock(changedBlock);
  await waitFor(() => { b.assertHealthy(); return b.block(changedBlock) === 0; }, 'shared block edit');
  const before = await request('/');
  assert.ok(Number.isFinite(before.server.wasm_memory_bytes) && before.server.wasm_memory_bytes > 0);
  console.log(`DO startup: ${before.startup_ms} ms; wasm memory with two players: ${before.server.wasm_memory_bytes} bytes`);
  await delay(200);
  assert.ok((await request('/')).server.ticks > before.server.ticks, 'Ticker stopped between events');
  savedPosition = { ...a.position, x: a.position.x + 0.25, z: a.position.z + 0.25 };
  a.move(savedPosition);
  await delay(300);
  assert.equal(a.block(changedBlock), 0, 'Edited block changed before checkpoint');
  assert.equal(b.block(changedBlock), 0, 'Second client lost the edit before checkpoint');
  await a.close(); await b.close(); clients.length = 0;
  const saved = await checkpoint(worker);
  console.log(`World saved at ${saved.saved_at}`);
} finally {
  await Promise.all(clients.map(client => client.close()));
  await worker.stop();
}

worker = await startWorker();
try {
  assert.equal((await request('/')).phase, 'idle');
  const a = await play('ProbeA');
  try {
    await waitFor(() => { a.assertHealthy(); worker.healthy(); return a.chunks.has(savedChunk); }, 'saved chunk after restart');
    assert.equal(a.block(changedBlock), 0, 'Edited block was regenerated instead of restored');
    assert.equal(a.position.x, savedPosition.x, 'Player X position was not restored');
    assert.equal(a.position.z, savedPosition.z, 'Player Z position was not restored');
  } finally { await a.close(); }
  await checkpoint(worker);
  worker.healthy();
} finally { await worker.stop(); }

console.log('PUMPKIN-DO-SQLITE-RESTART-OK');
