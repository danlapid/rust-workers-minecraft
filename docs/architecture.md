# Architecture

## Runtime and connections

The whole Worker is Rust: `worker-build --emscripten`, with the `worker` crate's
`experimental_tokio` feature, links the `pumpkin-do` bin for `wasm32-unknown-emscripten`
through emcc and emits the module Wrangler serves. The entrypoint's
`#[event(connect)]` handler reads a bounded Minecraft handshake over the
`worker::Socket` (`src/handshake.rs`): server-list requests are answered from
the object's metadata (`serverList`) without starting a server, and gameplay
connections are forwarded to the named `MinecraftWorld` object (declared under
`exports` in `wrangler.jsonc` and reached through `cloudflare:workers`'
`exports`) with its `connect`, writing the consumed bytes ahead and copying both directions
with Tokio. `fetch` (`#[event(fetch)]`) serves status.

`MinecraftWorld` is a `#[durable_object(connect)]`; its constructor mounts
`state.storage().as_raw()` and the server root awaits `Storage::sync`. The
macro exports the object's handlers with `experimental_tokio` (ambient): the
future runs on the thread's Tokio `LocalEventLoop` (tokio-rs/tokio#8484), the
current-thread scheduler and drivers with the host event loop as their wait,
and the export returns the Promise of its outcome. Nothing blocks or suspends.
`status` and `serverList` are plain `#[wasm_bindgen]` RPC methods. The first
`connect` after idle runs the server lifetime: it starts Pumpkin on the mounted
world, awaits the server's return after its final save, awaits `storage.sync()`,
and settles the connection's promise. Later connections and `status` calls enter
the same instance as ordinary calls while that future is parked. A connection
arriving while the server is starting or stopping waits for the next phase
change and then retries, so a client that connects as the previous server
saves and stops starts the next one. The runtime's epoll and timer waits register
with the host through Emscripten's Node backend (emscripten-core/emscripten#27547),
so a readiness or timer callback resumes the scheduler on the host loop.

Pumpkin binds its stock `TcpListener` on port 25565. Emscripten's Node socket
backend implements that with `net.BoundSocket`/`net.Server`, which workerd scopes
to the Durable Object's own port table. The object's `connect` routes each
inbound socket to that listener with `Socket::handle_as_node_connection`; the
accepted connection surfaces
through epoll readiness and Pumpkin's normal accept loop, reporting the bound
address and an unspecified peer.

One wasm instance serves the isolate, so one object hosts one server at a time;
`WORLD_NAME` selects it. When the last connection closes, the server root saves
players, level data and dirty chunks, then keeps the server running for the
`IDLE_TIMEOUT_SECONDS` reconnect window (`idle_deadline` in status); a new
connection cancels the shutdown. When the window expires the root calls
`stop_server`, Pumpkin performs its final save and returns, and `reset_stop`
clears the process-wide stop state so the next connection can start a fresh
server in the same instance. Operator settings ([configuration](configuration.md))
are read from the environment in the object's constructor.

Pumpkin's `single-threaded` feature runs chunk generation as Tokio tasks
and selects the async ticker. The same scheduler loop serves native builds, which
dispatch generation onto Rayon. Save batches await queue capacity; shutdown
drains results with an elapsed-time timeout before the final flush. Bounded CPU
tasks use regular Tokio tasks because `spawn_blocking` requires OS threads.

Structure templates store palette indices and share their block arrays between
the placement and query APIs. Block-entity NBT is shared until a placement needs
an owned copy. Generation chunks retain uniform or indexed sections; only the
active noise pass uses a temporary dense block buffer. This keeps the generation
neighborhood compact without changing its dependency rules. Emscripten grows
memory in 2 MiB increments, configured by `build.rs`, while retaining the 8 MiB
stack. See [memory measurements](memory-reduction.md).

## Filesystem

The world is a SQLite-backed filesystem: the object constructor mounts
`durable-object-fs`'s `LocalDOFilesystem(storage)` at `/data` through
`worker-fs-mount`, whose `node:fs` implementation routes that prefix to the
object's storage and every other path (stdio, `/tmp`) to workerd's own
filesystem. `src/workerd.js` rebinds Emscripten's `NODERAWFS` to it, resolves
relative paths against Emscripten's working directory (`/data`, distinct from
the isolate's `process.cwd()`), and disables the VFS's own permission checks
since the host enforces them. Every Pumpkin write is therefore durable as it
happens, under the object's transaction semantics; after the final save the
server awaits `storage.sync()` before the last connection is reported closed,
and status shows that time.

Logging goes to the process's stdout and stderr through `NODERAWFS`, which
Wrangler relays as `stdout:` and `stderr:` lines.

## Development interfaces

The supplied configuration listens on loopback TCP port 25565 and HTTP port 8787.
The world name comes from `WORLD_NAME` in `wrangler.jsonc`.

- `GET /health`: Worker readiness.
- `GET /`: phase, connection count, runtime statistics, the runtime start
  count, last shutdown save time, reconnect deadline, and effective settings.

Runtime statistics include Wasm capacity and allocator in-use/free/arena bytes.
Allocator counters include allocation metadata and unused container capacity;
Wasm capacity additionally includes static data, stack, and growth headroom.

Local databases live in `.data/workers/server/`. Integration tests use separate
storage under `.data/probes/` and restart Wrangler to verify restoration. Rust
panics, Rust error logs, and runtime crashes fail the integration test.

## Build constraints

- Use the pinned Rust toolchain and the worker-build from setup, which
  provisions Emscripten and the wasm-bindgen CLI.
- Keep static relocation, exnref exception handling on both the C and Rust
  sides, the 8 MiB stack, and memory growth. worker-build supplies the common
  codegen and link settings (including `--cfg tokio_unstable`; `LocalEventLoop`
  is unstable API); `build.rs` adds the application's own: `NODERAWFS`, the
  stack size, the growth step, and the JS library.
- Prefix Rust exports to avoid collisions with libc and Emscripten symbols.

[Dependency pins and patch maintenance](dependencies.md)
