import { exports } from 'cloudflare:workers';
import { PORT } from './pumpkin';
import { forwardSocket } from './socket';
export { MinecraftWorld } from './world';

export default {
  async connect(socket: Socket, env: Env): Promise<void> {
    const target = exports.MinecraftWorld.getByName(env.WORLD_NAME).connect(`world:${PORT}`, { allowHalfOpen: true });
    await forwardSocket(socket, target);
  },
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === '/health') return Response.json({ ready: true });
    const stub = exports.MinecraftWorld.getByName(env.WORLD_NAME);
    if (request.method === 'GET' && url.pathname === '/') return Response.json(await stub.status());
    return new Response('Not found', { status: 404 });
  },
} satisfies ExportedHandler<Env>;
