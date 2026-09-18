import assert from 'node:assert/strict';
import { test } from 'node:test';
import { StatusPackets, handshakeState, answerStatus } from '../worker/minecraft-status.ts';
import { handshake, frame, readVarint } from './protocol.mjs';

function stream(chunks) {
  return new ReadableStream({ start(c) { for (const chunk of chunks) c.enqueue(chunk); c.close(); } });
}
async function collect(readable) {
  const chunks = [];
  for await (const chunk of readable) chunks.push(chunk);
  return Buffer.concat(chunks);
}

test('fragmented login handshake and coalesced payload are forwarded byte for byte', async () => {
  const input = Buffer.concat([handshake(2), frame(0, Buffer.from('login bytes'))]);
  const packets = new StatusPackets(stream([input.subarray(0, 2), input.subarray(2, 7), input.subarray(7)]));
  assert.equal(handshakeState(await packets.read()).state, 2);
  assert.deepEqual(await collect(packets.replay()), input);
});

test('coalesced status request and ping return metadata and the exact ping payload', async () => {
  const nonce = Buffer.from('01234567');
  const packets = new StatusPackets(stream([Buffer.concat([handshake(1), frame(0), frame(1, nonce)])]));
  assert.equal(handshakeState(await packets.read()).protocol, 776);
  const output = [];
  await answerStatus(packets, new WritableStream({ write(bytes) { output.push(bytes); } }), async () => ({ players: { online: 2, max: 4 } }));
  const [length, offset] = readVarint(output[0]);
  const [jsonLength, jsonOffset] = readVarint(output[0], offset + 1);
  assert.equal(length, 1 + jsonOffset - offset - 1 + jsonLength);
  assert.equal(JSON.parse(output[0].subarray(jsonOffset).toString()).players.online, 2);
  assert.deepEqual(output[1], frame(1, nonce));
  await packets.cancel();
});

test('oversized, truncated and stalled pre-login packets are rejected', async () => {
  const oversized = new StatusPackets(stream([Buffer.from([0xff, 0x7f])]));
  await assert.rejects(oversized.read(), /size/);
  await oversized.cancel();
  const partial = new StatusPackets(stream([Buffer.from([5, 0])]));
  await assert.rejects(partial.read(), /Truncated/);
  await partial.cancel();
  const stalled = new StatusPackets(new ReadableStream(), 10);
  await assert.rejects(stalled.read(), /timed out/);
  await stalled.cancel();
  assert.throws(() => handshakeState(Buffer.from([0, 1, 127])), /handshake/);
  const invalid = handshake(7);
  assert.throws(() => handshakeState(invalid.subarray(readVarint(invalid)[1])), /state/);
});
