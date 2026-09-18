# Server configuration

Gameplay settings live in `wrangler.jsonc` under `vars`. Use string values.
Changing these settings restarts the local Worker, but does not require rebuilding
Rust. Invalid settings appear in the HTTP status response's `failure` field.

| Variable | Default | Meaning |
| --- | --- | --- |
| `WORLD_NAME` | `world` | Selects the Durable Object and its separate saved world. |
| `WORLD_SEED` | Unset | Seed for a new world; an existing world keeps its saved seed. |
| `VIEW_DISTANCE` | `4` | Maximum client view distance, from 2 to 32 chunks. |
| `SIMULATION_DISTANCE` | `3` | Simulation radius, from 2 to the configured view distance. |
| `MAX_PLAYERS` | `2` | Login limit, from 1 to 1,000; higher values need load testing. |
| `COMPRESSION_THRESHOLD` | `512` | Compress packets at or above this byte count; `-1` disables compression. |
| `COMPRESSION_LEVEL` | `1` | Compression level, from 0 to 9; 1 favors speed. |
| `IDLE_TIMEOUT_SECONDS` | `10` | Reconnect window after a checkpoint, from 0 to 30 seconds; 0 stops immediately. |
| `MOTD` | `Minecraft on Cloudflare Workers` | Description displayed in the Minecraft server list. |

The accepted ranges describe valid protocol settings, not a supported capacity
for every world. Chunk generation, separate player locations, and world content
can substantially increase memory usage. A larger view distance does not imply
a larger simulation distance is needed.

The default view is now 4 with simulation distance 3. Pumpkin's
[memory optimizations](memory-reduction.md#exploration-comparison-september-12-2026)
made room for the larger view: a two-player exploration probe commanding 512
blocks of travel each completed at 113.81 MiB of Wasm capacity. That is one
workload, not an unrestricted exploration guarantee.

To try a larger view while measuring your world's memory usage:

```sh
npm run dev -- --var VIEW_DISTANCE:5 --var SIMULATION_DISTANCE:3
```

For packet debugging without compression:

```sh
npm run dev -- --var COMPRESSION_THRESHOLD:-1
```

The server still runs at 20 ticks per second in Survival with Normal difficulty.
It uses offline authentication; the example's listeners bind to loopback.

## Status and reconnects

Server-list requests are answered before forwarding a gameplay connection to
Pumpkin. They report the configured description and player limit, and the current
player count, without creating a Wasm instance. Status requests have a bounded
packet size and a five-second handshake/ping deadline.

After the final gameplay connection closes, the server saves and synchronizes
storage. It then keeps the runtime available for the reconnect window. A new
connection cancels the pending shutdown. When the window expires, it performs
its final save and shuts down.

The HTTP status endpoint reports `runtime_starts`, `checkpointed_at`,
`idle_deadline`, the effective `settings`, and runtime memory/tick statistics.
`phase: "saving"` means a checkpoint is in progress. During the reconnect window,
`phase` is `running`, `connections` is zero, and `idle_deadline` is set. Wait for
`phase: "idle"` before stopping Wrangler so the final shutdown also completes.

Changing configuration reloads the Worker. Wait for players to leave and the
world to become idle before changing settings on a world you care about.

## Measuring a configuration

The experience probe starts an isolated world with a fixed seed. It measures
initial view delivery, stationary play, and synthetic movement in different
directions, recording traffic, Wasm capacity, allocator usage, status latency,
and observed tick rate. It writes `experience.json` under `.data/probes/`.

```sh
npm run test:experience -- --view 3 --simulation 3 --players 2 --compression=512
npm run test:experience -- --view 4 --simulation 3 --players 2 --compression=512
npm run test:experience -- --view 3 --simulation 3 --players 2 --compression=-1
```

Use the same seed, workload, and build when comparing settings. These are local
synthetic probes, not player-capacity guarantees. Wasm capacity excludes
JavaScript allocations, and sampling can miss short-lived peaks.

Tests can use separate loopback ports while the playable server is running:

```sh
TEST_TCP_PORT=25566 TEST_HTTP_PORT=8788 TEST_INSPECTOR_PORT=9230 npm test
TEST_TCP_PORT=25566 TEST_HTTP_PORT=8788 TEST_INSPECTOR_PORT=9230 npm run test:experience -- --view 4
```

`TEST_INSPECTOR_PORT` separates the probe's debugger listener from another local
Wrangler instance. The probes fail if their chosen ports are occupied and only
stop processes they started. Playable world databases are never used by the probes.

## Local comparison (September 12, 2026)

This historical sweep predates the palette, light, carving-mask, and pathfinder
optimizations. It used the same size-optimized build, seed, and 32 movement
steps per player. Each step commanded four blocks of movement, with players
moving in different directions. Results are single runs:

| View / simulation | Players | Compression | Initial views | Peak Wasm capacity | Received traffic |
| --- | ---: | --- | ---: | ---: | ---: |
| 3 / 3 | 2 | Off | 6.29 s | 123.75 MiB | 19.62 MB |
| 3 / 3 | 2 | Level 1 | 6.30 s | 121.75 MiB | 2.16 MB |
| 4 / 3 | 2 | Level 1 | 7.43 s | 129.75 MiB | 2.76 MB |
| 3 / 3 | 4 | Level 1 | 6.19 s | 159.75 MiB | 4.33 MB |
| 2 / 2 | 2 | Level 1 | 5.17 s | 107.75 MiB | 1.50 MB |

Compression reduced traffic by about 89% in the comparable view-distance-3
runs, with essentially unchanged initial load time. The larger view and
four-player runs left insufficient headroom for a standard Workers isolate;
Wasm capacity does not include JavaScript memory. The temporary 2/2 preset was
rerun after adding the acknowledged checkpoint and reached 107.81 MiB.

`opt-level=3` failed to build: the pinned Binaryen `wasm-metadce` process exited
with SIGBUS. The working `opt-level="s"` profile is retained; no speed improvement
is claimed for the failed experiment.

[Machine-readable measurements](configuration-results.json) include the settings,
Wasm hashes, sampled status latency, and observed tick rate. A run interrupted
by a development-server reload was excluded. The early-checkpoint change was
validated separately with 179 native world tests and the full restart suite,
including terminating the test Worker before the reconnect deadline.

A longer run commanded 512 blocks of travel per player and reached **125.75 MiB
of Wasm capacity**, despite the smaller preset. A separate seed reached 113.75
MiB. Both completed, but the long run leaves very little space for JavaScript
inside the standard isolate memory budget. These controls reduce demand; they
do not establish unrestricted exploration or a general two-player capacity
bound. The subsequent [memory work](memory-reduction.md#exploration-comparison-september-12-2026)
reduced allocation at the same gameplay settings and replaced that temporary
preset with the current 4/3 defaults. The original measurements remain in
`configuration-results.json`; the newer [exploration results](memory-exploration-results.json)
record the optimized builds and unsuccessful attempts separately.
