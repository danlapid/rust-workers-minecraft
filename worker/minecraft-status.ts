import { Buffer } from 'node:buffer';

// Matches the Java versions supported by the pinned Pumpkin checkout.
export const MIN_PROTOCOL = 4;
export const MAX_PROTOCOL = 776;
export const VERSION_NAME = '1.7.2-26.2';
const MAX_PACKET = 1024;

export function varint(value: number): Buffer {
  const bytes = [];
  do {
    const byte = value & 127;
    value >>>= 7;
    bytes.push(value ? byte | 128 : byte);
  } while (value);
  return Buffer.from(bytes);
}

export function readVarint(bytes: Buffer, offset = 0): [number, number] | null {
  let value = 0;
  for (let i = 0; i < 5; i++) {
    if (offset + i >= bytes.length) return null;
    const byte = bytes[offset + i];
    if (i === 4 && (byte & 0xf0)) throw new Error('Invalid VarInt');
    value |= (byte & 127) << (7 * i);
    if (!(byte & 128)) return [value >>> 0, offset + i + 1];
  }
  throw new Error('Invalid VarInt');
}

function requiredVarint(bytes: Buffer, offset: number): [number, number] {
  const result = readVarint(bytes, offset);
  if (!result) throw new Error('Truncated handshake');
  return result;
}

export function handshakeState(packet: Buffer): { protocol: number; state: number } {
  const [id, a] = requiredVarint(packet, 0);
  const [protocol, b] = requiredVarint(packet, a);
  const [hostLength, c] = requiredVarint(packet, b);
  if (id !== 0 || hostLength > MAX_PACKET || c + hostLength + 2 >= packet.length) {
    throw new Error('Invalid handshake');
  }
  const [state, end] = requiredVarint(packet, c + hostLength + 2);
  if (end !== packet.length) throw new Error('Invalid handshake length');
  if (state < 1 || state > 3) throw new Error('Invalid handshake state');
  return { protocol, state };
}

function frame(id: number, body: Buffer = Buffer.alloc(0)): Buffer {
  const content = Buffer.concat([varint(id), body]);
  return Buffer.concat([varint(content.length), content]);
}

/** Read only the handshake/status prefix; gameplay bytes are forwarded unchanged. */
export class StatusPackets {
  private reader: ReadableStreamDefaultReader<Uint8Array>;
  private buffer = Buffer.alloc(0);
  private deadline: number;
  private lastFrame = Buffer.alloc(0);

  constructor(readable: ReadableStream<Uint8Array>, timeoutMs = 5000) {
    this.reader = readable.getReader();
    this.deadline = Date.now() + timeoutMs;
  }

  async read(): Promise<Buffer | null> {
    for (;;) {
      const length = readVarint(this.buffer);
      if (length) {
        const [size, start] = length;
        if (size < 1 || size > MAX_PACKET) throw new Error('Invalid handshake/status packet size');
        if (this.buffer.length >= start + size) {
          this.lastFrame = this.buffer.subarray(0, start + size);
          const packet = this.buffer.subarray(start, start + size);
          this.buffer = this.buffer.subarray(start + size);
          return packet;
        }
      }
      const remaining = this.deadline - Date.now();
      if (remaining <= 0) throw new Error('Handshake/status timed out');
      let timer: ReturnType<typeof setTimeout> | undefined;
      try {
        const next = await Promise.race([
          this.reader.read(),
          new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new Error('Handshake/status timed out')), remaining);
          }),
        ]);
        if (next.done) {
          if (this.buffer.length) throw new Error('Truncated handshake/status packet');
          return null;
        }
        this.buffer = Buffer.concat([this.buffer, next.value]);
      } finally { clearTimeout(timer); }
    }
  }

  replay(): ReadableStream<Uint8Array> {
    const reader = this.reader;
    const prefix = Buffer.concat([this.lastFrame, this.buffer]);
    this.buffer = Buffer.alloc(0);
    return new ReadableStream<Uint8Array>({
      start(controller) { controller.enqueue(prefix); },
      async pull(controller) {
        const { value, done } = await reader.read();
        if (done) { reader.releaseLock(); controller.close(); }
        else controller.enqueue(value);
      },
      async cancel(reason) { await reader.cancel(reason); reader.releaseLock(); },
    });
  }

  async cancel(): Promise<void> {
    await this.reader.cancel().catch(() => undefined);
    this.reader.releaseLock();
  }
}

export async function answerStatus(
  packets: StatusPackets,
  writable: WritableStream<Uint8Array>,
  status: () => Promise<object>,
): Promise<void> {
  const writer = writable.getWriter();
  try {
    const request = await packets.read();
    if (!request || request.length !== 1 || request[0] !== 0) throw new Error('Invalid status request');
    const json = Buffer.from(JSON.stringify(await status()));
    await writer.write(frame(0, Buffer.concat([varint(json.length), json])));
    const ping = await packets.read();
    if (ping === null) return;
    if (ping.length !== 9 || ping[0] !== 1) throw new Error('Invalid status ping');
    await writer.write(frame(1, ping.subarray(1)));
  } finally { writer.releaseLock(); }
}
