import createPumpkin from '../target/workers/wasm32-unknown-emscripten/release/pumpkin-do.js';
import pumpkinWasm from '../target/workers/wasm32-unknown-emscripten/release/pumpkin_do.wasm';
import { createWorldRuntime } from './filesystem';
import type { ServerSettings } from './config';

/** Bind the server API to its instance's filesystem and async context once. */
export async function startPumpkin(storage: DurableObjectStorage, settings: ServerSettings) {
  const { instance, run } = await createWorldRuntime(storage, options => createPumpkin({
    ...options,
    instantiateWasm(imports, receive) {
      const instance = new WebAssembly.Instance(pumpkinWasm, imports);
      receive(instance);
      return instance.exports;
    },
    printErr: console.error,
  }));
  await run(() => instance.pumpkin_start(JSON.stringify(settings)));
  return {
    connect: (socket: Socket) => run(() => instance.pumpkin_connect(socket)),
    save: () => run(() => instance.pumpkin_save()),
    stop: () => run(() => instance.pumpkin_shutdown()),
    status: () => run(() => instance.pumpkin_status()),
  };
}
