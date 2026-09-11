# Dependency patches

`scripts/setup.sh` applies these patches to the revisions in
[dependency pins](../docs/dependencies.md).

Each patch starts with a commit subject, a description of the change and its
motivation, and the base commit. `git apply` skips this header and applies the
diff below it. Preserve the description when regenerating the diff.

| Patch | Checkout | Purpose |
| --- | --- | --- |
| `pumpkin-emscripten.patch` | `.work/pumpkin` | Headless optional subsystems, single-threaded runtime and blocking-task helper, shared async chunk scheduler with backpressure and timed draining, unified ticker, and platform guards |
| `pumpkin-memory.patch` | `.work/pumpkin` | Shared indexed structure templates, immutable block-entity NBT, paletted generation chunks, and a temporary dense noise buffer |

Setup applies `pumpkin-emscripten.patch` before `pumpkin-memory.patch`. The two patches use the
same pinned base and currently modify separate files, so each can also be applied
and checked independently.

Both Pumpkin patches exclude its Cargo.lock; the application's root Cargo.lock
locks the embedded build. Setup checks reverse application before reapplying each
patch.
