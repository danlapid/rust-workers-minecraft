# Minecraft on Workers

A [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) Minecraft Java server, compiled
from Rust to WebAssembly and hosted in a Cloudflare Durable Object. Workers accepts
the TCP connection; Pumpkin runs the game; a mounted SQLite filesystem stores the
world.

![Minecraft Java 26.2 multiplayer alongside Wrangler running the Pumpkin server](docs/images/minecraft-on-workers.png)

```text
Minecraft → Workers TCP ingress → MinecraftWorld → Pumpkin + SQLite
```

## Try it

Linux and macOS build hosts are supported. Install Git,
[rustup](https://rustup.rs/), Python 3.11+, and Node 24+ (26 recommended). From a checkout of this repository:

```sh
npm ci
bash scripts/setup.sh
npm run dev
```

Setup installs the pinned Rust toolchain, fetches and patches Pumpkin under
`.work/`, and installs worker-build, which provisions Emscripten and
wasm-bindgen itself. `npm run dev` compiles Pumpkin with
`worker-build --emscripten` and starts Wrangler. The first build takes several
minutes; later builds use Cargo's cache.

The runtime support this needs (`net.Server` routing into Durable Objects and
the `node:fs` fixes listed in [dependency pins](docs/dependencies.md#host-tools))
is in workerd main. Until Wrangler's bundled workerd catches up, point Miniflare
at a build of main: `MINIFLARE_WORKERD_PATH=/path/to/workerd npm run dev`.

Connect **Minecraft Java 26.2** to **`localhost:25565`**. Status is available at
**http://localhost:8787/**.

The example uses offline mode, with encryption and compression disabled. Local
listeners bind to loopback. `WORLD_NAME` in [wrangler.jsonc](wrangler.jsonc) selects
the Durable Object; server settings are in [src/config.rs](src/config.rs).

World data lives under `.data/workers/server/`, written through to the Durable
Object's SQLite storage as Pumpkin saves. After the last connection closes, the
server saves and stops; the next connection starts it on the same world. Wait for
status to report `phase: "idle"` before stopping Wrangler so the final save
completes.

## Build and contribute

```sh
npm run build        # Compile the Worker without starting Wrangler
npm test             # Two-player gameplay, a persistent block edit, and restart
npm run test:scheduler
```

The tests use separate data under `.data/probes/`. They verify that two clients
observe the same block edit and that the edit and player position survive restart.

See [development](docs/development.md), [architecture](docs/architecture.md), and
[dependency pins](docs/dependencies.md) for build details and patch maintenance.

## Deploy

After setup, build the runtime and deploy the Worker:

```sh
npm run build
npx wrangler deploy
```

Deployment uses standard SQLite-backed Durable Objects with Workers TCP ingress.
See [memory usage](docs/memory-reduction.md) for the optimizations and measurements.
Provision the public TCP endpoint separately and route it to this Worker's
`connect` handler. The listener in `wrangler.jsonc` is for local development.

## Credits and license

Built on [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin),
[workers-rs](https://github.com/cloudflare/workers-rs),
[Tokio](https://github.com/tokio-rs/tokio), [Emscripten](https://emscripten.org/),
[wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen), and
[Guy Bedford's Rust/Emscripten work](https://github.com/guybedford).

Licensed under [GPL-3.0-only](LICENSE), consistent with Pumpkin. The
[original MIT notice](LICENSES/rust-workers-minecraft-MIT.txt) is retained for
inherited code. Dependencies retain their own licenses. This is an unofficial
project and is not affiliated with Mojang or Microsoft.
