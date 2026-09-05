import assert from 'node:assert/strict';
import net from 'node:net';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { HOST, PORT, PROTOCOL, handshake, frame, varint, mcString, readVarint, packets, blockPosition, readBlockPosition, chunkBlock, sectionUpdates } from './protocol.mjs';

export async function waitFor(check, description, milliseconds = 60_000) {
  const deadline = Date.now() + milliseconds;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value;
    await delay(100);
  }
  throw new Error(`Timed out waiting for ${description}`);
}

async function socket() {
  const stream = net.connect({ host: HOST, port: PORT });
  stream.setNoDelay(true);
  stream.setTimeout(90_000, () => stream.destroy(new Error('Minecraft client timed out')));
  await new Promise((resolve, reject) => {
    stream.once('connect', resolve);
    stream.once('error', reject);
  });
  return stream;
}

export async function status() {
  const stream = await socket();
  stream.write(handshake(1));
  stream.write(frame(0));
  try {
    for await (const { id, payload } of packets(stream)) {
      assert.equal(id, 0);
      const [length, offset] = readVarint(payload);
      const status = JSON.parse(payload.subarray(offset, offset + length).toString());
      assert.equal(status.version.protocol, PROTOCOL);
      return status;
    }
    throw new Error('No status response');
  } finally { stream.destroy(); }
}

/** A small vanilla-protocol client that remains connected after receiving chunks. */
export async function play(name) {
  const stream = await socket();
  const closed = new Promise(resolve => stream.once('close', resolve));
  const uuid = createHash('md5').update(`OfflinePlayer:${name}`).digest();
  uuid[6] = (uuid[6] & 0x0f) | 0x30;
  uuid[8] = (uuid[8] & 0x3f) | 0x80;
  const chunks = new Map();
  const seenNames = new Set();
  const blockUpdates = new Map();
  let state = 'login';
  let teleported = false;
  let position;
  let loaded = false;
  let closing = false;
  let failure;
  let readyResolve, readyReject;
  const ready = new Promise((resolve, reject) => { readyResolve = resolve; readyReject = reject; });
  stream.write(handshake(2));
  stream.write(frame(0, Buffer.concat([mcString(name), uuid])));
  const reading = (async () => {
    for await (const { id, payload } of packets(stream)) {
      if (state === 'login') {
        if (id === 0) throw new Error(`${name}: login rejected`);
        if (id === 1 || id === 3) throw new Error('Probe requires offline mode with compression disabled');
        if (id === 2) {
          stream.write(frame(3));
          // Vanilla clients opt into the server listing through Client Information.
          stream.write(frame(0, Buffer.concat([mcString('en_us'), Buffer.from([3, 0, 1, 0x7f, 1, 0, 1])])));
          state = 'configuration';
        }
      } else if (state === 'configuration') {
        if (id === 2) throw new Error(`${name}: disconnected during configuration`);
        if (id === 0x0e) stream.write(frame(7, varint(0)));
        if (id === 4 || id === 5) stream.write(frame(id, payload));
        if (id === 3) { stream.write(frame(3)); state = 'play'; }
      } else {
        if (id === 0x20) throw new Error(`${name}: disconnected during Play`);
        if (id === 0x54) {
          for (const update of sectionUpdates(payload)) {
            blockUpdates.set(`${update.x},${update.y},${update.z}`, update.state);
          }
        }
        if (id === 0x08) {
          const position = readBlockPosition(payload);
          blockUpdates.set(`${position.x},${position.y},${position.z}`, readVarint(payload, 8)[0]);
        }
        if (id === 0x2c) stream.write(frame(0x1c, payload));
        if (id === 0x48) {
          const [teleport, offset] = readVarint(payload);
          position = { x: payload.readDoubleBE(offset), y: payload.readDoubleBE(offset + 8), z: payload.readDoubleBE(offset + 16) };
          stream.write(frame(0, varint(teleport)));
          teleported = true;
        }
        if (id === 0x2d) {
          assert.ok(payload.length > 8);
          const x = payload.readInt32BE(0), z = payload.readInt32BE(4);
          chunks.set(`${x},${z}`, payload);
          if (!loaded) { stream.write(frame(0x2c)); loaded = true; }
        }
        // Player-info packets carry profile names. Preserve observations for
        // the two-client test without requiring an entity/render implementation.
        if (id === 0x46) { // Player Info Update
          for (const candidate of ['ProbeA', 'ProbeB']) {
            if (payload.includes(mcString(candidate))) seenNames.add(candidate);
          }
        }
        if (id === 0x0b) {
          const rate = Buffer.alloc(4);
          rate.writeFloatBE(8);
          stream.write(frame(0x0b, rate));
          if (teleported && chunks.size > 0) readyResolve();
        }
      }
    }
    if (!closing) throw new Error(`${name}: server closed the connection`);
  })().catch(error => {
    if (!closing) { failure = error; readyReject(error); }
  });
  let timer;
  try {
    await Promise.race([
      ready,
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${name}: no Play chunk within 90 seconds`)), 90_000); }),
    ]);
  } catch (error) { closing = true; stream.destroy(); throw error; }
  finally { clearTimeout(timer); }
  return {
    chunks, seenNames,
    block(position) {
      const update = blockUpdates.get(`${position.x},${position.y},${position.z}`);
      if (update !== undefined) return update;
      const chunk = chunks.get(`${Math.floor(position.x / 16)},${Math.floor(position.z / 16)}`);
      return chunk && chunkBlock(chunk, position);
    },
    async breakBlock(position) {
      const action = (status, sequence) => stream.write(frame(0x29, Buffer.concat([
        varint(status), blockPosition(position), Buffer.from([1]), varint(sequence),
      ])));
      action(0, 1);
      await delay(1000);
      action(2, 2);
      await waitFor(() => {
        if (failure) throw failure;
        return blockUpdates.get(`${position.x},${position.y},${position.z}`) === 0;
      }, 'block destruction acknowledgement');
    },
    get position() { return position; },
    move(next) {
      const payload = Buffer.alloc(25);
      payload.writeDoubleBE(next.x, 0);
      payload.writeDoubleBE(next.y, 8);
      payload.writeDoubleBE(next.z, 16);
      payload[24] = 1; // On ground
      stream.write(frame(0x1e, payload));
    },
    assertHealthy() { if (failure) throw failure; },
    async close() {
      closing = true;
      stream.end();
      const timer = setTimeout(() => stream.destroy(), 3000);
      await closed;
      clearTimeout(timer);
      await reading;
    },
  };
}
