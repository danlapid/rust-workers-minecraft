# Memory usage

The optimized server runs on standard SQLite-backed Durable Objects.

The memory optimizations are maintained in
[`pumpkin-memory.patch`](../patches/pumpkin-memory.patch), separately from the
Emscripten embedding patch. Setup applies both to the pinned Pumpkin revision.

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

The default is now view 4/simulation 3 with two players. The long four-player
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
