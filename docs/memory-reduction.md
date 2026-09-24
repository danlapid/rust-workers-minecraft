# Memory usage

The optimized server runs on standard SQLite-backed Durable Objects.

Storage and cache optimizations are maintained in
[`pumpkin-memory.patch`](../patches/pumpkin-memory.patch). Scheduler and network
queue changes live in the embedding patch alongside their runtime integration.
Setup applies both to the pinned Pumpkin revision.

Structure templates share indexed block arrays and store block-state descriptions
once per palette. Block-entity NBT is shared until placement needs a mutable copy.
Generation chunks retain uniform or indexed sections; a temporary dense buffer
keeps the active noise pass efficient. Emscripten grows memory in 2 MiB increments
while retaining the 8 MiB stack.

Exploration profiling added four further changes:

- Pack small chunk-palette indices into 1, 2, 4, or 8 bits instead of one byte
  each. Palette growth still falls back to dense storage when needed; Minecraft's
  separate network and disk packing is unchanged.
- Share uniform initialized light arrays and detach on writes. Compact arrays
  after initial light propagation, when many have become uniform. Initialized
  light remains distinct from missing light, preserving packet masks and saves.
- Allocate carving-mask words only as carving reaches them, retaining every bit
  and the original dimension bounds.
- Let pathfinder heaps grow when searching. Each idle navigator previously
  reserved 1,024 nodes before using any of them; search limits are unchanged.

`LightContainer::Full` now holds `Arc<[u8]>` instead of `Box<[u8]>`. This changes
the Rust enum's public payload type and needs consideration when upstreaming;
the serialized light bytes remain unchanged.

## Exploration comparison (September 12, 2026)

A matched comparison used four clients, view/simulation distance 3, seed
`1789195700909824175`, and level-1 compression above 512 bytes. Each client sent
32 four-block movement steps in a different direction, spaced 500 ms apart,
with five seconds stationary before and after movement. The baseline already
included the earlier template and generation-chunk optimizations. These runs
used Node 26, Wrangler 4.129.0, and workerd 1.20260903.1 on macOS arm64.

| Measurement | Baseline | Baseline repeat | Optimized |
| --- | ---: | ---: | ---: |
| Peak sampled Wasm capacity | 161.75 MiB | 159.75 MiB | 111.81 MiB |
| Peak sampled allocated heap | 130.13 MiB | 129.35 MiB | 81.58 MiB |
| Time to receive initial views | 6.95 s | 7.87 s | 6.26 s |
| Observed exploration ticks/second | 15.28 | 15.26 | 15.41 |

At identical gameplay settings, Wasm capacity fell about **30%** and allocated
heap about **37%**. Tick performance was similar; these individual runs do not
establish a CPU speedup.

An allocator walk of the baseline's Wasm memory found 31.55 MiB in 4 KiB palette
buffers, 27.61 MiB in 2 KiB light buffers (about 17.6 MiB held uniform values),
7.04 MiB in carving masks, and 8.44 MiB in idle pathfinder buffers. Heap
backreferences identified the latter as vectors with length 1 and capacity 1,024.
These measurements guided the changes rather than reducing gameplay settings.

Additional optimized runs checked the larger view and longer movement:

| View / simulation | Players | Movement steps each | Seed | Peak Wasm | Result |
| --- | ---: | ---: | --- | ---: | --- |
| 4 / 3 | 2 | 128 | Same | 113.81 MiB | Passed |
| 4 / 3 | 2 | 32 | `1789200079352125165` | 99.81 MiB | Passed |
| 4 / 3 | 4 | 32 | Same | 125.81 MiB | Passed |
| 3 / 3 | 4 | 128 | Same | 159.81 MiB | Passed on retry |

The configuration initially chosen after these measurements used view 4/simulation
3 with a two-player limit. The current limit is 20; that capacity has not been
load-tested. The long four-player
result is still too large for the standard isolate budget, and even the
two-player results do not establish a general capacity bound. Movement counts
describe commands sent by synthetic clients, not verified terrain traversal.
Wasm capacity excludes JavaScript allocations; sampling can miss transient peaks.

Some attempts failed: the first long optimized four-player run disconnected a
client, and several baseline and optimized attempts failed during startup or
initial view delivery. One baseline log also reported an esbuild deadlock.
Later runs used a separate inspector port and completed, but the cause of the
failures was not established. Failed attempts are retained in the
[measurement report](memory-exploration-results.json), with intermediate builds
identified separately; their partial peaks are not comparable to completed runs.

The final source passed 186 native world tests, 101 protocol tests, and the
pathfinder heap growth/ordering test. The full integration suite passed at the
new 4/3 defaults, including a shared block edit and restoration of the player
position and edited chunk after restarting Wrangler.

## Cache lifetime and outgoing data (September 22, 2026)

This pass kept view distance 4, simulation distance 3, and the configured player
limit of 20. Four clients each commanded 128 four-block movement steps in
different directions. The baseline includes the earlier memory optimizations
and the matching LLVM 24/Binaryen 131 backend.

| Measurement | Baseline | Baseline repeat | Optimized |
| --- | ---: | ---: | ---: |
| Peak sampled Wasm capacity | 173.81 MiB | 175.88 MiB | 155.94 MiB |
| Peak sampled allocated heap | 129.86 MiB | 130.98 MiB | 113.19 MiB |
| Initial views | 8.63 s | 8.82 s | 7.49 s |
| Observed exploration ticks/second | 14.31 | 14.17 | 14.92 |

That is approximately **10–11% less Wasm capacity** and **13–14% less allocated
heap** at the same settings. The optimized run received 1,876 distinct chunks
across the clients, compared with 1,277 in the baseline repeat. The lower memory
use did not come from delivering less terrain. Timing results remain individual
observations, not a statistical CPU benchmark.

With **20 nearby clients** and no movement, the optimized build peaked at
**83.94 MiB** of Wasm capacity versus **91.69 MiB** in both baseline runs. All
clients received their initial 117 chunks. Initial view delivery took 9.24 s
optimized and 8.64–8.73 s baseline. A second seed's optimized four-client long
run completed at 161.94 MiB; its baseline attempts failed, so it is not a paired
comparison. These results do not establish a 20-player exploration capacity.

The changes address retained memory and large bursts of outgoing data:

- Release large generation-graph arrays once no queued or running task can hold
  a key. Clear stale holder keys before reusing the graph.
- After the structure-start cache passes 2,048 entries, periodically remove
  entries that active generation no longer references. Keep shared collector
  identity intact, including concurrent cache misses.
- Limit pooled density-buffer payloads to 4 MiB per thread while retaining
  reusable buffers that fit.
- Give each Java client's queued chunk payloads a 2 MiB byte budget. Reserve
  capacity for a whole chunk batch before marking chunks sent. When capacity is
  unavailable, chunks stay pending for retry. A single larger chunk can occupy
  the whole budget so valid packets still make progress.
- Trim serialized packet capacity and release raw packet data after writing its
  frame. Keep completion acknowledgements until the writer flushes. Asynchronous
  resends serialize small groups and share the same batch-ordering guard.

The [complete report](memory-retention-results.json) includes unsuccessful
attempts and intermediate builds. Some heap captures caused client timeouts;
stopped-world dumps were excluded. Several baseline and candidate runs ended
with Wrangler proxy errors. Their cause was not established. An additional
dependency-pruning experiment was left out because it showed no clear benefit
over the selected implementation.

Native validation covered 191 world tests and six packet/chunk tests, including
atomic batch reservation, retrying unsent chunks, collector identity, and pool
accounting. The integration suite verifies player and edited-chunk restoration.

To repeat the workloads with the current build:

```sh
TEST_TCP_PORT=25566 TEST_HTTP_PORT=8788 TEST_INSPECTOR_PORT=9241 npm run test:experience -- --players 4 --max-players 20 --view 4 --simulation 3 --steps 128
TEST_TCP_PORT=25566 TEST_HTTP_PORT=8788 TEST_INSPECTOR_PORT=9241 npm run test:experience -- --players 20 --max-players 20 --view 4 --simulation 3 --steps 0
```

## Earlier stationary comparison (September 5, 2026)

A local comparison on September 5, 2026 used the same world seed, two nearby
Java 26.2 clients, and view/simulation distance 3. Both clients received 81 chunks,
then remained stationary for 30 seconds. The host used Node 26.8.1, Wrangler
4.129.0, and workerd 1.20260903.1 on macOS arm64.

| Measurement | Before | After |
| --- | ---: | ---: |
| Peak observed Wasm capacity | 197.06 MiB | 87.75 MiB |
| Settled allocated heap | 148.57 MiB | 54.22 MiB |
| Time to receive both views | 4.96 s | 5.62 s |

Wasm capacity fell 55.5%. The final heap snapshot totaled **97.90 MB
(93.36 MiB)**, including the Wasm allocation and the rest of the captured graph.
The Wasm capacity remained unchanged through checkpointing, then its backing
store was released after the world stopped.

These are individual local measurements, not a general memory cap or a
statistical timing benchmark. Other seeds, distant players, and exploration can
retain more data. Heap snapshots collect garbage and do not measure every
transient JavaScript allocation. Process RSS is a separate measurement.

## Measuring changes

Run `npm run test:memory` for a fresh world with a fixed seed, or supply a seed
with `npm run test:memory -- --seed 1789200079352125165`. See
[development](development.md) for the probe's output and sampling limits.

Status reports `wasm_memory_bytes`, `heap_allocated_bytes`, `heap_free_bytes`, and
`heap_arena_bytes`. Allocator counters include metadata and spare capacity inside
containers. Wasm capacity also includes static data, the stack, and growth
headroom. Use a DevTools heap snapshot to account for JavaScript allocations.

The earlier optimization was validated with 114 native generation tests, including
structure/template round trips, and the full integration suite's shared block
edit and player/block restoration. Future changes must continue to pass
`npm test` with `PUMPKIN-DO-SQLITE-RESTART-OK`.
