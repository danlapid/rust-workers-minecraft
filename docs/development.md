# Development

Follow the [README setup instructions](../README.md#try-it) to install the pinned
Rust toolchain, provision the patched Pumpkin checkout and install worker-build.
The initial Pumpkin compilation takes several minutes. Later builds use Cargo's
cache. worker-build owns the common codegen and link settings; `build.rs` adds
the application's own and the JS library (`src/workerd.js`), tracking changes to
it. `.cargo/config.toml` repeats the codegen cfgs so `cargo check` sees them.
Wrangler runs `worker-build --emscripten --release` as its build command
and serves `build/index.js`.

```sh
npm run build          # Compile the production Worker
npm test               # Full Wrangler integration suite
npm run test:scheduler # Native scheduler, chunk-queue and shutdown regressions
npm run test:memory    # Two-player memory probe on a fresh, fixed-seed world
npm run test:experience # Configuration and movement/traffic probe
```

Tests fail if their loopback ports (25565 and 8787 by default) are occupied; set
`TEST_TCP_PORT` and `TEST_HTTP_PORT` to use others. They start and
stop only their own Wrangler process groups. Their databases and logs live in
`.data/probes/`; playable worlds are separate under `.data/workers/server/`.

`npm test` must finish with `PUMPKIN-DO-SQLITE-RESTART-OK`. It runs the unit
tests (`tests/*.test.mjs`: protocol framing), then
two-player gameplay with compression, server-list pings that must not start the
runtime, the player limit, a real block edit, a reconnect during the idle window,
and player/block restoration after a restart from the SQLite-mounted world.

The memory probe waits for both clients to receive their configured view, observes 30 seconds
of stationary play, then checkpoints. Each run creates a fresh isolated world
with a fixed seed; use the same seed when comparing builds. See
[configuration](configuration.md) for the exploration probe:

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
