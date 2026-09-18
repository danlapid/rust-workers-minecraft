# Dependency patches

`scripts/setup.sh` applies these patches to the revisions in
[dependency pins](../docs/dependencies.md).

Each patch starts with a commit subject, a description of the change and its
motivation, and the base commit. `git apply` skips this header and applies the
diff below it. Preserve the description when regenerating the diff.

| Patch | Checkout | Purpose |
| --- | --- | --- |
| `pumpkin-emscripten.patch` | `.work/pumpkin` | Headless optional subsystems, single-threaded runtime and blocking-task helper, shared async chunk scheduler with backpressure and timed draining, acknowledged checkpoints, unified ticker, and platform guards |
| `pumpkin-memory.patch` | `.work/pumpkin` | Shared indexed templates and block-entity NBT, compact generation chunks and palettes, shared light arrays, and lazy carving-mask/pathfinder allocation |
| `wasm-bindgen-emscripten-closures.patch` | `.work/wasm-bindgen` | Escape generated closure-finalizer postsets as JavaScript strings |
| `workers-rs-emscripten-toolchain.patch` | `.work/workers-rs` | Match the pinned wasm-bindgen ABI, provide the Emscripten Tokio promise adapter, leave initialization/recovery to the host, and flush pending writes before socket shutdown |
| `wasm-streams-rlib.patch` | `.work/wasm-streams` | Omit the standalone cdylib when embedding the stream adapter in Emscripten |

The wasm-streams patch is a downstream workaround for this application's static
Emscripten build. Removing its `cdylib` output upstream could break consumers of
that artifact; passing Rust and browser tests does not establish compatibility
for those consumers. Keep both upstream outputs while investigating a narrower
linker or toolchain fix.
See the [Emscripten linking investigation](../docs/emscripten-linking.md) for the
configuration conflict and binding-generation failures this workaround hides.

`pumpkin-emscripten.patch` includes the injected-stream connection entry point used
by the DO. Setup applies it before `pumpkin-memory.patch`. The two patches use the
same pinned base and currently modify separate files, so each can also be applied
and checked independently.

The memory patch covers `crates/pumpkin-world/src/generation/`,
`crates/pumpkin-world/src/chunk/format/mod.rs`,
`crates/pumpkin-world/src/chunk/palette.rs`,
`crates/pumpkin-world/src/lighting/engine.rs`, and
`crates/pumpkin/src/entity/ai/pathfinder/binary_heap.rs`. The regeneration commands
in [dependency pins](../docs/dependencies.md#updating-a-patch) exclude these paths
from the embedding patch.

Both Pumpkin patches exclude its Cargo.lock; the application's root Cargo.lock
locks the embedded build. Setup checks reverse application before reapplying each
patch.

The [wasm-bindgen side-module review branch](https://github.com/wasm-bindgen/wasm-bindgen/compare/main...danlapid:fix/emscripten-function-got)
and [merged Walrus fix](https://github.com/wasm-bindgen/walrus/pull/320) track the
upstream toolchain work. These changes are not applied by setup to the pinned
runtime; see the [linking investigation](../docs/emscripten-linking.md).
