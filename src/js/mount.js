// The world's filesystem: worker-fs-mount's node:fs implementation routes paths
// under a mount to the Durable Object's SQLite storage and everything else to
// workerd's node:fs. Emscripten's NODERAWFS is rebound to it (src/workerd.js).
import { LocalDOFilesystem } from 'durable-object-fs/local';
import { mount, unmount } from 'worker-fs-mount';
import * as fs from 'worker-fs-mount/fs-sync';

export const ROOT = '/data';

// The mount table is per isolate and an isolate outlives an object instance
// (and may host several), so each constructor takes the mount over: one wasm
// instance serves one object at a time, and the server runs only while the
// constructing object holds it.
export function mountStorage(storage) {
  unmount(ROOT);
  mount(ROOT, new LocalDOFilesystem(storage));
  globalThis.__pumpkin_fs = fs;
}
