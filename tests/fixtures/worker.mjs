import { DurableObject, exports } from 'cloudflare:workers';
export { StartupFailure } from './startup-failure.mjs';
import createModule from '../../target/workers/wasm32-unknown-emscripten/release/filesystem-probe.js';
import wasm from '../../target/workers/wasm32-unknown-emscripten/release/filesystem_probe.wasm';
import { createWorldRuntime } from '../../worker/filesystem';
import { forwardSocket } from '../../worker/socket';

export class FilesystemTest extends DurableObject {
  runtime;

  getRuntime() {
    return this.runtime ??= createWorldRuntime(this.ctx.storage, options => createModule({
      ...options,
      instantiateWasm(imports, receive) {
        const instance = new WebAssembly.Instance(wasm, imports);
        receive(instance);
        return instance.exports;
      },
    }));
  }

  async connect(socket) {
    void socket.closed.catch(() => undefined);
    const runtime = await this.getRuntime();
    await runtime.run(() => runtime.instance.echo(socket));
  }

  async status() {
    const runtime = await this.getRuntime();
    return { timer: await runtime.run(() => runtime.instance.tick_after(20)) };
  }

  async filesystemProbe(value) {
    const runtime = await this.getRuntime();
    return runtime.run(() => runtime.instance.filesystem_probe(value));
  }
}

export default {
  async connect(socket, env) {
    const target = exports.FilesystemTest.getByName(env.WORLD_NAME).connect('world:25565', { allowHalfOpen: true });
    await forwardSocket(socket, target);
  },
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/health') return Response.json({ ready: true });
    if (url.pathname === '/startup-failure') {
      return Response.json(await exports.StartupFailure.getByName('failure').verify());
    }
    const stub = exports.FilesystemTest.getByName(url.searchParams.get('world') ?? env.WORLD_NAME);
    if (request.method === 'POST' && url.pathname === '/filesystem-probe') {
      return Response.json({ previous: await stub.filesystemProbe(await request.text()) });
    }
    return Response.json(await stub.status());
  },
};
