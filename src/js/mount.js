// The world's filesystem: worker-fs-mount's node:fs implementation routes paths
// under a mount to the Durable Object's SQLite storage and everything else to
// workerd's node:fs. Emscripten's NODERAWFS is rebound to it (src/workerd.js).
import { LocalDOFilesystem } from 'durable-object-fs/local';
import { mount } from 'worker-fs-mount';
import * as fs from 'worker-fs-mount/fs-sync';

export const ROOT = '/data';

export function mountStorage(storage) {
  mount(ROOT, new LocalDOFilesystem(storage));
  globalThis.__pumpkin_fs = fs;
}
