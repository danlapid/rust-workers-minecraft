import assert from 'node:assert/strict';
import net from 'node:net';
import { readFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { status, play, waitFor } from './client.mjs';
import { testServer } from './server.mjs';
import { PORT } from './protocol.mjs';

const { startWorker, request, checkpoint } = await testServer({ vars: { IDLE_TIMEOUT_SECONDS: '5', MAX_PLAYERS: '2' } });

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

async function echo() {
  const socket = net.connect({ host: '127.0.0.1', port: PORT });
  socket.setTimeout(15000, () => socket.destroy(new Error('Echo timed out')));
  const payload = Buffer.from(Array.from({ length: 65536 }, (_, index) => index & 255));
  const chunks = [];
  let length = 0;
  try {
    await new Promise((resolve, reject) => {
      socket.once('connect', resolve);
      socket.once('error', reject);
    });
    for (let offset = 0; offset < payload.length; offset += 997) socket.write(payload.subarray(offset, offset + 997));
    for await (const chunk of socket) {
      chunks.push(chunk);
      length += chunk.length;
      if (length >= payload.length) break;
    }
    assert.ok(Buffer.concat(chunks).equals(payload), `TCP echo differed: received ${length} of ${payload.length} bytes`);
  } finally { socket.destroy(); }
}

async function marker(world, value) {
  return (await request(`/filesystem-probe?world=${world}`, { method: 'POST', body: value })).previous;
}

let worker = await startWorker(true);
try {
  await Promise.all([echo(), echo()]);
  assert.equal((await request('/')).timer, true);
  const failure = await request('/startup-failure');
  assert.equal(failure.starting.phase, 'starting');
  assert.equal(failure.failed.phase, 'failed');
  assert.equal(failure.failed.failure, 'Expected startup failure');
  assert.deepEqual(failure.repeated, failure.failed);
  const checkpointFailure = await request('/checkpoint-failure');
  assert.equal(checkpointFailure.rejected, true);
  assert.equal(checkpointFailure.after.phase, 'failed');
  assert.equal(checkpointFailure.after.checkpointed_at, null);
  assert.equal(checkpointFailure.after.idle_deadline, null);
  assert.equal(await marker('isolation-a', 'alpha'), '');
  assert.equal(await marker('isolation-b', 'beta'), '');
  assert.equal(await marker('isolation-a', 'alpha-2'), 'alpha');
  worker.healthy();
} finally { await worker.stop(); }
worker = await startWorker(true);
try {
  assert.equal(await marker('isolation-a', 'alpha-3'), 'alpha-2');
  assert.equal(await marker('isolation-b', 'beta-2'), 'beta');
  worker.healthy();
} finally { await worker.stop(); }
console.log('Filesystem, socket, and isolation tests passed');

let savedChunk, savedPosition, changedBlock;
worker = await startWorker();
const clients = [];
try {
  assert.equal((await request('/')).runtime_starts, 0);
  assert.equal((await status({ fragmented: true })).version.protocol, 776);
  assert.equal((await status()).players.max, 2);
  assert.equal((await request('/')).runtime_starts, 0, 'Status ping started Wasm');
  const a = await play('ProbeA'); clients.push(a);
  const b = await play('ProbeB'); clients.push(b);
  await waitFor(() => {
    a.assertHealthy(); b.assertHealthy(); worker.healthy();
    return a.seenNames.has('ProbeB') && b.seenNames.has('ProbeA');
  }, 'both players to see each other');
  assert.equal((await status()).players.online, 2);
  assert.equal(a.traffic.compressionThreshold, 512);
  assert.ok(a.traffic.decodedBytes > a.traffic.wireBytes, 'Compression did not reduce traffic');
  await assert.rejects(play('ProbeExtra'), /login rejected/);
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
  const warm = await checkpoint(worker, { stopped: false });
  const reconnected = await play('ProbeA'); clients.push(reconnected);
  const resumed = await request('/');
  assert.equal(resumed.runtime_starts, warm.runtime_starts, 'Reconnect restarted Wasm');
  await delay(5500); // Outlive the first disconnect's timer while connected.
  reconnected.assertHealthy(); worker.healthy();
  assert.ok((await request('/')).server.ticks > resumed.server.ticks, 'Reconnected world stopped ticking');
  await reconnected.close(); clients.length = 0;
  const saved = await checkpoint(worker, { stopped: false, after: warm.checkpointed_at });
  // Stop this test worker before the grace timer fires, proving the earlier
  // checkpoint alone is sufficient for player and block restoration.
  console.log(`World checkpoint completed at ${saved.checkpointed_at}`);
} finally {
  await Promise.all(clients.map(client => client.close()));
  await worker.stop();
}

worker = await startWorker(false, { COMPRESSION_THRESHOLD: '-1' });
try {
  assert.equal((await request('/')).phase, 'idle');
  const a = await play('ProbeA');
  try {
    assert.equal(a.traffic.compressionThreshold, null);
    await waitFor(() => { a.assertHealthy(); worker.healthy(); return a.chunks.has(savedChunk); }, 'saved chunk after restart');
    assert.equal(a.block(changedBlock), 0, 'Edited block was regenerated instead of restored');
    assert.equal(a.position.x, savedPosition.x, 'Player X position was not restored');
    assert.equal(a.position.z, savedPosition.z, 'Player Z position was not restored');
  } finally { await a.close(); }
  await checkpoint(worker);
  worker.healthy();
} finally { await worker.stop(); }

console.log('PUMPKIN-DO-SQLITE-RESTART-OK');
