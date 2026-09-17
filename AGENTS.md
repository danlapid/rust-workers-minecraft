# Working on Rust Workers Minecraft

Read [README.md](README.md), [architecture](docs/architecture.md), and
[dependency pins](docs/dependencies.md) before changing the runtime.

## Project layout

- `src/`: the Worker: TCP ingress, the Durable Object, the SQLite filesystem
  mount (`js/mount.js`), Pumpkin configuration, and the Emscripten JS library
  (`workerd.js`).
- `tests/`: protocol clients and the integration test.
- `scripts/setup.sh`: provision pinned sources and build the toolchain.
- `scripts/{build,serve,test}.sh`: build, run, and validate the Workers server.

Use Node 24+ (26 recommended) and the pinned Rust toolchain. Run `npm test` after
build/runtime changes; it must print `PUMPKIN-DO-SQLITE-RESTART-OK` after verifying
player and chunk restoration.
Scripts bind to loopback and fail if their ports are occupied. Do not kill another
process to free a port.

## Reproducibility and data

Changes to the patched Pumpkin checkout must be reflected in `patches/`. Keep unpatched checkouts unmodified; update pins for
upstream changes. Preserve the unified ticker and cooperative scheduler unless
the task requires a runtime change. Keep the event-loop model: exports
return promises and nothing blocks or suspends; do not reintroduce JSPI, old
Tokio/libc networking patches, or a JS-side driver.

Worlds under `.data/` are user data; do not remove or copy them into commits.
Playable databases live in `.data/workers/server/`; tests use `.data/probes/`.

## Public repository and Git safety

Never commit `.work/`, `target/`, `.data/`, generated JS/wasm, credentials, private
CA material, account IDs, or non-public infrastructure names/URLs. Retain upstream
license notices and the GPL license for this Pumpkin-linked project.

Never commit, push, rebase, reset --hard, or force-push without explicit approval.
Show the proposed diff/commands and wait for confirmation. Prefer additive changes.
