import { AsyncLocalStorage } from 'node:async_hooks';
import { LocalDOFilesystem } from 'durable-object-fs/local';
import { mount, withMounts } from 'worker-fs-mount';
import * as nodeFs from 'worker-fs-mount/fs-sync';

/** Each wasm instance keeps one mount context across requests and Tokio callbacks. */
export function createWorldRuntime<T>(
  storage: DurableObjectStorage,
  create: (options: { nodeFs: typeof nodeFs; cwd: string }) => Promise<T>,
) {
  return withMounts(async () => {
    mount('/data', new LocalDOFilesystem(storage));
    const run = AsyncLocalStorage.snapshot();
    return { instance: await create({ nodeFs, cwd: '/data' }), run };
  });
}
