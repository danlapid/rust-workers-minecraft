# Architecture

## Runtime and connections

The Worker reads a bounded Minecraft handshake and answers server-list requests
from Durable Object metadata. Gameplay streams are forwarded unchanged to the
named Durable Object using the platform's stream-piping API. The object lazily creates an Emscripten module and passes a
workers-rs `Socket` to Pumpkin's injected connection entry point.

Each object owns separate wasm memory, Rust statics, filesystem descriptors, and
one hosted Tokio runtime, supplied by workers-rs. Pumpkin's `single-threaded` feature selects cooperative
inline chunk generation and the async ticker. The same scheduler loop serves
native builds, which dispatch generation onto Rayon. Save batches await queue
capacity; shutdown drains results with an elapsed-time timeout before the final
flush. Bounded CPU tasks use
regular Tokio tasks because the pinned fork's public `spawn_blocking` still
requires OS threads. JSPI supplies stack suspension, not threads.

Structure templates store palette indices and share their block arrays between
the placement and query APIs. Block-entity NBT is shared until a placement needs
an owned copy. Generation chunks retain uniform or indexed sections; only the
active noise pass uses a temporary dense block buffer. This keeps the generation
neighborhood compact without changing its dependency rules. Emscripten grows
memory in 2 MiB increments, configured by `build.rs`, while retaining the 8 MiB
stack. See [memory measurements](memory-reduction.md).

Chunk palettes pack small indices into 1, 2, 4, or 8 bits without changing their
wire format. Uniform initialized light arrays are shared; writes detach the
affected array. Carving masks allocate only through their highest used word,
and idle pathfinders reserve nodes when a search begins.

Generation-graph storage is released after all tasks finish, with stale keys
cleared before reuse. Density-buffer pools retain at most 4 MiB of payload per
thread. Structure-start caches periodically drop idle entries while preserving
collectors still referenced by generation.

Java chunk delivery reserves queue slots and byte credit for complete batches.
When the 2 MiB per-client chunk budget is full, unsent chunks remain pending for
a later tick. A single larger chunk can use the whole budget. Raw packet buffers
are released after frame writes; completion notifications still wait for flush.

Emscripten factory mode (`MODULARIZE=1`) isolates module instances. Wrangler
provides the compiled wasm through `instantiateWasm`; the generated host glue uses
Workers' Node compatibility APIs. The SDK's standalone-loader initialization and
recovery hook is disabled for this embedding.

## Filesystem and checkpoints

The host mounts `LocalDOFilesystem(ctx.storage)` from durable-object-fs at `/data`.
Emscripten's `NODERAWFS` routes Rust filesystem calls through worker-fs-mount's
synchronous Node API, including descriptors, positional I/O, rename, and sync.
World metadata, chunks, entities, players, and configuration use this mount.
Logging goes to the host console.

Each wasm instance retains its own mount context across requests and Tokio
callbacks using `AsyncLocalStorage.snapshot()`. Descriptors opened at startup
therefore remain valid until that runtime shuts down, without exposing one world's
mount to another object.

The Emscripten compatibility library in `worker/fs-library.js` supplies the mounted Node
filesystem to the factory: the generated loader's runtime `createRequire` calls
cannot use Wrangler's build-time aliases. It also initializes file flags from
public `fs.constants` and implements synchronous `fd_sync`. Paths resolve against
the wasm instance's working directory (initially `/data`), without changing the
isolate-wide Node process directory.

The upstream `entries` table stores metadata, and `file_pages` stores contents in
64 KiB rows. Descriptor writes update only affected pages. Truncation supports
sparse files, and file writes and native rename use SQLite transactions. Whole-file
reads still require a buffer large enough for the result.

After the final gameplay connection leaves, Pumpkin saves and the object awaits
`storage.sync()`. A configurable reconnect window keeps that runtime available;
new connections wait for the save and cancel its pending shutdown. When the
window expires, Pumpkin performs the final save and shutdown. Checkpoint failures
remain visible in status.
This preserves issued writes; terminating a server with active clients can still
lose changes in Pumpkin's in-memory caches.

## Development interfaces

The supplied configuration listens on loopback TCP port 25565 and HTTP port 8787.
The world name comes from `WORLD_NAME` in `wrangler.jsonc`.

- `GET /health`: Worker readiness.
- `GET /`: phase, connection count, runtime statistics, and last checkpoint time.

Runtime statistics include Wasm capacity, allocator in-use/free/arena bytes, and
mean tick execution time. Lifecycle status includes the runtime start count,
checkpoint time, reconnect deadline, and effective configuration.
Allocator counters include allocation metadata and unused container capacity;
Wasm capacity additionally includes static data, stack, and growth headroom.

The public status path reads no filesystem internals and remains available when
startup fails. Checkpointing is internal to the connection lifecycle. The
`worker/pumpkin.ts` facade binds exports to the module context once, so the
Durable Object deals with `connect`, `stop`, and `status` methods.

Local databases live in `.data/workers/server/`. Integration tests use separate
storage under `.data/probes/` and restart Wrangler to verify restoration. The echo
and large-file test fixture has its own Worker configuration and wasm binary; it
is excluded from the application bundle.

Current workerd versions can report `Network connection lost` when TCP clients
disconnect. Rust panics, Rust error logs, and runtime crashes fail the integration
test.

## Build constraints

- Use the pinned Rust nightly and matching wasm-bindgen CLI from setup.
- Keep unwind semantics, the 8 MiB stack, and memory growth. The current
  static-relocation override conflicts with shared-library output; see the
  [Emscripten linking investigation](emscripten-linking.md).
- Pass Emscripten link settings through rustc so they do not affect C compilation.
- Keep the compatibility initializer linked even when Rust does not call `fd_sync`;
  `build.rs` configures this and tracks the library as a build input.
- Prefix Rust exports to avoid collisions with libc and Emscripten symbols.
- The synchronous SQLite mount supplies `fd_sync`; checkpoint completion separately
  awaits Durable Object storage synchronization.

[Dependency pins and patch maintenance](dependencies.md)
