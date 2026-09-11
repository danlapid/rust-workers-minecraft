# Development

Follow the [README setup instructions](../README.md#try-it) to install the pinned
Rust toolchain, provision dependency sources and Emscripten, and install the
wasm-bindgen CLI.
The initial Pumpkin compilation takes several minutes. Later builds use Cargo's
cache. `build.rs` owns the Emscripten library arguments and tracks changes to the
compatibility library.

```sh
npm run build          # Compile the production server
npm run typecheck      # Regenerate Worker types and check TypeScript
npm test               # Full Wrangler integration suite
npm run test:scheduler # Native queue/backpressure and shutdown regressions
npm run test:memory    # Two-player memory probe on a fresh, fixed-seed world
```

Tests fail if their loopback ports (25565 and 8787) are occupied. They start and
stop only their own Wrangler process groups. Their databases and logs live in
`.data/probes/`; playable worlds are separate under `.data/workers/server/`.

`npm test` must finish with `PUMPKIN-DO-SQLITE-RESTART-OK`. It checks filesystem
isolation, synchronous descriptors, large files, failed-startup status, two-player
gameplay, a real block edit, and player/block restoration after a restart.

The memory probe waits for both clients to receive 81 chunks, observes 30 seconds
of stationary play, then checkpoints. Each run creates a fresh isolated world
with a fixed seed; use the same seed when comparing builds:

```sh
npm run test:memory -- --seed 1789200079352125165
```

The probe writes `memory.json` under `.data/probes/`, recording the seed, Wasm and
host-glue hashes, observed capacity, and sampled allocator usage. Sampling ends
before checkpointing and can miss short-lived allocations. These numbers exclude
JavaScript memory; use an inspector snapshot to check the combined footprint.

The test harness sets `WORLD_SEED` to known terrain with a stable spawn block.
The optional Worker variable also makes new playable worlds reproducible; existing
worlds keep the seed in their saved metadata. Without it, new worlds use a random
seed as before.

Node is a build and test tool here; the server runs in Workers. If your shell
selects an older Node version, use a version manager or set
`NODE=/absolute/path/to/node` when invoking the shell scripts.

Read [dependency pins](dependencies.md) before changing a checkout. Every change
to a patched dependency must be reflected in `patches/`. The embedded build uses
the root Cargo.lock. Native Pumpkin tests use the dependency's own workspace.
