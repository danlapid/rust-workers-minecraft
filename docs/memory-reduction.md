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

## Recorded comparison

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

The optimization was validated with 114 native generation tests, including
structure/template round trips, and the full integration suite's shared block
edit and player/block restoration. Future changes must continue to pass
`npm test` with `PUMPKIN-DO-SQLITE-RESTART-OK`.
