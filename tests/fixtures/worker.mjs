import { DurableObject, exports } from 'cloudflare:workers';
import { handleAsNodeConnection } from 'cloudflare:node';
export { StartupFailure } from './startup-failure.mjs';
import createModule from '../../target/workers/wasm32-unknown-emscripten/release/filesystem-probe.js';
import wasm from '../../target/workers/wasm32-unknown-emscripten/release/filesystem_probe.wasm';
import { createWorldRuntime } from '../../worker/filesystem';
import { forwardSocket } from '../../worker/socket';

const PORT = 25565;

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
    })).then(async runtime => {
      await new Promise((ready, reject) => {
        runtime.run(() => runtime.instance.echo_server(ready)).catch(reject);
      });
      return runtime;
    });
  }

  async connect(socket) {
    void socket.closed.catch(() => undefined);
    await this.getRuntime();
    await handleAsNodeConnection(socket);
  }

  async status() {
    await this.getRuntime();
    return { listening: true };
  }

  async filesystemProbe(value) {
    const runtime = await this.getRuntime();
    return runtime.run(() => runtime.instance.filesystem_probe(value));
  }
}

export default {
  async connect(socket, env) {
    const target = exports.FilesystemTest.getByName(env.WORLD_NAME).connect(`world:${PORT}`, { allowHalfOpen: true });
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
