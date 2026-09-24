import assert from 'node:assert/strict';
import { test } from 'node:test';
import { frame, packets, varint } from './protocol.mjs';

test('compression starts immediately after negotiation, including coalesced packets', async () => {
  const payload = Buffer.alloc(4096, 97);
  const incoming = Buffer.concat([frame(3, varint(512)), frame(42, payload, 512), frame(43, Buffer.from('small'), 512)]);
  const compression = { threshold: null };
  const decoded = [];
  for await (const packet of packets([incoming], compression)) {
    if (packet.id === 3) compression.threshold = 512;
    else decoded.push(packet);
  }
  assert.deepEqual(decoded, [{ id: 42, payload }, { id: 43, payload: Buffer.from('small') }]);
  assert.ok(incoming.length < payload.length);
});

test('compressed framing rejects inflated lengths above the packet limit', async () => {
  const body = varint(8 * 1024 * 1024 + 1);
  await assert.rejects(async () => {
    for await (const _ of packets([Buffer.concat([varint(body.length), body])], { threshold: 512 })) {}
  }, /exceeds limit/);
});
