# Dependency patches

`scripts/setup.sh` applies these patches to the revisions in
[dependency pins](../docs/dependencies.md).

Each patch starts with a commit subject, a description of the change and its
motivation, and the base commit. `git apply` skips this header and applies the
diff below it. Preserve the description when regenerating the diff.

| Patch | Checkout | Purpose |
| --- | --- | --- |
| `pumpkin-emscripten.patch` | `.work/pumpkin` | Headless optional subsystems, single-threaded runtime and blocking-task helper, shared async chunk scheduler with backpressure, timed draining and idle graph release, byte-bounded outgoing chunk batches, unified ticker, restartable stop signal, and platform guards |
| `worker-fs-mount-open-mode.patch` | `node_modules/worker-fs-mount` (via `npm install`) | Mask numeric `openSync`/`fchmodSync` modes to `0o7777` like Node, so Emscripten's `S_IFREG \| perms` creates files |
| `pumpkin-memory.patch` | `.work/pumpkin` | Shared indexed structure templates, immutable block-entity NBT, paletted generation chunks, a temporary dense noise buffer, packed palettes with copy-on-write light arrays, on-demand carving masks, bounded density pools, and pruned structure caches |

Patches whose paths start with `node_modules/` are applied by
`scripts/patch-packages.mjs` from npm's `postinstall`; the rest by setup.
Setup applies `pumpkin-emscripten.patch` before `pumpkin-memory.patch`. The two patches use the
same pinned base and currently modify separate files, so each can also be applied
and checked independently.

Both Pumpkin patches exclude its Cargo.lock; the application's root Cargo.lock
locks the embedded build. Setup checks reverse application before reapplying each
patch.
