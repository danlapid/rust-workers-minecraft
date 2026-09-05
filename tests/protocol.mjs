// Minimal, uncompressed Java 26.2 framing for this demo's smoke probe.
export const PROTOCOL = 776;
export const HOST = '127.0.0.1';
export const PORT = 25565;

export function varint(value) {
  const bytes = [];
  do {
    const byte = value & 0x7f;
    value >>>= 7;
    bytes.push(value ? byte | 0x80 : byte);
  } while (value);
  return Buffer.from(bytes);
}

export function readVarint(buffer, offset = 0) {
  let value = 0;
  for (let i = 0; i < 5; i++) {
    if (offset + i >= buffer.length) return null;
    const byte = buffer[offset + i];
    value |= (byte & 0x7f) << (7 * i);
    if (!(byte & 0x80)) return [value >>> 0, offset + i + 1];
  }
  throw new Error('Invalid VarInt: more than five bytes');
}

export function mcString(value) {
  const bytes = Buffer.from(value, 'utf8');
  return Buffer.concat([varint(bytes.length), bytes]);
}

export function frame(id, payload = Buffer.alloc(0)) {
  const body = Buffer.concat([varint(id), payload]);
  return Buffer.concat([varint(body.length), body]);
}

export function handshake(state) {
  const port = Buffer.alloc(2);
  port.writeUInt16BE(PORT);
  return frame(0, Buffer.concat([varint(PROTOCOL), mcString(HOST), port, varint(state)]));
}

export async function* packets(socket) {
  let buffer = Buffer.alloc(0);
  for await (const data of socket) {
    buffer = Buffer.concat([buffer, data]);
    while (buffer.length) {
      const length = readVarint(buffer);
      if (!length) break;
      const [size, start] = length;
      if (size === 0 || size > 8 * 1024 * 1024) throw new Error('Invalid packet size');
      if (buffer.length < start + size) break;
      const body = buffer.subarray(start, start + size);
      buffer = buffer.subarray(start + size);
      const id = readVarint(body);
      if (!id) throw new Error('Missing packet ID');
      yield { id: id[0], payload: body.subarray(id[1]) };
    }
  }
  if (buffer.length) throw new Error('Connection ended mid-packet');
}

export function blockPosition({ x, y, z }) {
  const value = (BigInt.asUintN(26, BigInt(x)) << 38n)
    | (BigInt.asUintN(26, BigInt(z)) << 12n) | BigInt.asUintN(12, BigInt(y));
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64BE(value);
  return bytes;
}

export function readBlockPosition(bytes) {
  const value = bytes.readBigUInt64BE();
  return {
    x: Number(BigInt.asIntN(26, value >> 38n)),
    y: Number(BigInt.asIntN(12, value)),
    z: Number(BigInt.asIntN(26, value >> 12n)),
  };
}

/** Read one block from an uncompressed Java 26.2 overworld chunk. */
export function chunkBlock(chunk, { x, y, z }) {
  if (y < -64 || y >= 320) throw new RangeError('Block outside the overworld');
  let offset = 8;
  const int = () => {
    const result = readVarint(chunk, offset);
    if (!result) throw new Error('Truncated chunk');
    offset = result[1];
    return result[0];
  };
  const heightmaps = int();
  for (let i = 0; i < heightmaps; i++) { int(); const length = int(); offset += length * 8; }
  const size = int();
  const end = offset + size;
  if (end > chunk.length) throw new Error('Truncated chunk sections');
  const palette = (entries, indirectBits) => {
    const bits = chunk[offset++];
    if (bits === 0) { const value = int(); return () => value; }
    if (bits > 16) throw new Error(`Invalid palette width: ${bits}`);
    const values = bits <= indirectBits ? Array.from({ length: int() }, int) : null;
    const perWord = Math.floor(64 / bits);
    const start = offset;
    offset += Math.ceil(entries / perWord) * 8;
    if (offset > end) throw new Error('Palette exceeds section data');
    return index => {
      const word = chunk.readBigUInt64BE(start + Math.floor(index / perWord) * 8);
      const value = Number((word >> BigInt((index % perWord) * bits)) & ((1n << BigInt(bits)) - 1n));
      if (values && value >= values.length) throw new Error('Invalid palette index');
      return values ? values[value] : value;
    };
  };
  const section = Math.floor((y + 64) / 16);
  for (let i = 0; i <= section; i++) {
    offset += 4; // Non-air block count and fluid count.
    const block = palette(4096, 8);
    if (i === section) return block(((y & 15) << 8) | ((z & 15) << 4) | (x & 15));
    palette(64, 3);
  }
}

export function* sectionUpdates(payload) {
  const section = payload.readBigUInt64BE();
  const x = Number(BigInt.asIntN(22, section >> 42n)) * 16;
  const z = Number(BigInt.asIntN(22, section >> 20n)) * 16;
  const y = Number(BigInt.asIntN(20, section)) * 16;
  const count = readVarint(payload, 8);
  if (!count || count[0] > 4096) throw new Error('Invalid section update count');
  let offset = count[1];
  for (let i = 0; i < count[0]; i++) {
    let value = 0n;
    for (let shift = 0; ; shift += 7) {
      if (shift >= 70 || offset >= payload.length) throw new Error('Invalid block-update VarLong');
      const byte = payload[offset++];
      value |= BigInt(byte & 127) << BigInt(shift);
      if (!(byte & 128)) break;
    }
    yield {
      x: x + Number((value >> 8n) & 15n),
      y: y + Number(value & 15n),
      z: z + Number((value >> 4n) & 15n),
      state: Number(value >> 12n),
    };
  }
}
