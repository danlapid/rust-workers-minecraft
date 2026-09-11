import { handleAsNodeConnection } from 'cloudflare:node';
import createPumpkin from '../target/workers/wasm32-unknown-emscripten/release/pumpkin-do.js';
import pumpkinWasm from '../target/workers/wasm32-unknown-emscripten/release/pumpkin_do.wasm';
import { createWorldRuntime } from './filesystem';

export const PORT = 25565;

/** Start the server in its instance's filesystem context and resolve once it is listening. */
export async function startPumpkin(storage: DurableObjectStorage) {
  const { instance, run } = await createWorldRuntime(storage, options => createPumpkin({
    ...options,
    instantiateWasm(imports, receive) {
      const instance = new WebAssembly.Instance(pumpkinWasm, imports);
      receive(instance);
      return instance.exports;
    },
    printErr: console.error,
  }));
  // The single JSPI activation runs the whole server lifetime, suspending
  // whenever Tokio parks. It settles after `stop` completes the final save, or
  // rejects if the server fails.
  let finished!: Promise<void>;
  await new Promise<void>((ready, reject) => {
    finished = run(() => instance.pumpkin_run(ready));
    finished.catch(reject);
  });
  return {
    finished,
    connect: (socket: Socket) => handleAsNodeConnection(socket),
    stop: () => { instance.pumpkin_stop(); return finished; },
    status: () => instance.pumpkin_status(),
  };
}
