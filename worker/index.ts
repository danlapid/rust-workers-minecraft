import { exports } from 'cloudflare:workers';
import { forwardSocket } from './socket';
import { StatusPackets, handshakeState, answerStatus } from './minecraft-status';
export { MinecraftWorld } from './world';

export default {
  async connect(socket: Socket, env: Env): Promise<void> {
    void socket.closed.catch(() => undefined);
    const packets = new StatusPackets(socket.readable);
    let forwarded = false;
    try {
      const handshake = await packets.read();
      if (!handshake) return;
      const { protocol, state } = handshakeState(handshake);
      const world = exports.MinecraftWorld.getByName(env.WORLD_NAME);
      if (state === 1) {
        await answerStatus(packets, socket.writable, () => world.serverList(protocol));
      } else {
        const target = world.connect('world:25565', { allowHalfOpen: true });
        const readable = packets.replay();
        forwarded = true;
        await forwardSocket(socket, target, readable);
      }
    } catch (error) {
      // Invalid or interrupted pre-login traffic never starts a game runtime.
      console.debug('Minecraft connection ended:', String(error));
    } finally {
      if (!forwarded) await packets.cancel();
      await socket.close().catch(() => undefined);
    }
  },
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === '/health') return Response.json({ ready: true });
    const stub = exports.MinecraftWorld.getByName(env.WORLD_NAME);
    if (request.method === 'GET' && url.pathname === '/') return Response.json(await stub.status());
    return new Response('Not found', { status: 404 });
  },
} satisfies ExportedHandler<Env>;
