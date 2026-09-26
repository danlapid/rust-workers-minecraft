# Dependency sources and pins

A clean checkout is self-contained after `bash scripts/setup.sh`. The patched
Pumpkin checkout and the worker-build CLI live under `.work/` (ignored);
worker-build provisions its own Emscripten and wasm-bindgen CLI under
`~/.cache/worker-build`. Both Cargo and npm dependency graphs are locked.

## Pinned sources

| Component | Source / ref | Commit or tag | Local changes or purpose |
| --- | --- | --- | --- |
| pumpkin | [master](https://github.com/Pumpkin-MC/Pumpkin) | `b5b9b9d7010e793806a83c495af223c67e1d35ee` | Checkout under `.work/pumpkin`: headless embedding, shared async scheduler, restartable stop signal, compact templates/generation chunks, and build-script fixes |
| tokio | [guybedford/tokio](https://github.com/guybedford/tokio) | tag `1.53.1-cf.emscripten` | `LocalEventLoop` (tokio-rs/tokio#8484) and `net` over epoll on Emscripten; a `[patch.crates-io]` git dependency; unmodified |
| emscripten | [guybedford/emscripten](https://github.com/guybedford/emscripten) | tag `6.0.10-cf.emscripten` | The 6.0.10 release plus emscripten-core/emscripten#27547 (epoll listeners on the host loop) and #27742 (async DNS lookup). worker-build installs emsdk 6.0.10 and applies these as its bundled patches |

The Pumpkin patches are relative to the pinned commit above. Setup checks
reverse application before applying a patch and refuses to repin a modified
checkout.

Pumpkin has two patches: `pumpkin-emscripten.patch` for embedding/runtime support,
followed by `pumpkin-memory.patch` for compact templates and generation chunks.
They currently modify separate files and are both based on the pinned upstream
commit.

## Cargo-managed forks

| Crate | Source | Purpose |
| --- | --- | --- |
| mio | https://github.com/guybedford/mio tag `1.2.3-cf.emscripten` | Emscripten epoll selector (tokio-rs/mio#1969) |
| libc | https://github.com/rust-lang/libc branch `libc-0.2` | Emscripten epoll bindings, unreleased |
| rayon, rayon-core | https://github.com/guybedford/rayon branch `fallback-spawn` | rayon-rs/rayon#1323: the single-threaded fallback wakes the host to run spawned jobs |
| ring | https://github.com/guybedford/ring branch `emscripten` | getrandom-backed `SystemRandom` on Emscripten |

The root Cargo.toml applies these overrides and the local checkouts; the
wasm-bindgen crates (0.2.129, with `#[wasm_bindgen(experimental_tokio)]`) and
wasm-streams 0.7 come from crates.io. Cargo.lock pins the full application
graph, including the branch commits.

## Host tools

- Rust: `beta` channel (1.99; `OwnedFd::try_clone` on Emscripten), target
  `wasm32-unknown-emscripten`; setup installs both through rustup. rustc needs a
  larger compile-thread stack for pumpkin-data's generated tables; the scripts
  set `RUST_MIN_STACK`.
- worker-build 0.8.7 (cloudflare/workers-rs#1061 released): installed by setup
  with `cargo install`, matching the `worker` crate version in Cargo.toml, into
  `.work/bin/`. It provisions emsdk 6.0.10 with
  its bundled Emscripten patches and the matching wasm-bindgen CLI under
  `~/.cache/worker-build`, drives cargo and emcc with the common link settings,
  wraps the exports into the entrypoint and Durable Object classes, and emits
  `build/`. `EMSCRIPTEN`/`EMSDK` select a local toolchain instead.
- Node: 24+; 26 recommended and selected in CI. Used for build tools and tests.
- Python: 3.11+ (Emscripten scripts, emsdk and TOML parsing).
- workerd 1.20260925.1 (Wrangler 4.141.0's bundled version, also pinned directly
  in `package.json`); 1.20260918.1 was the first release with `net.Server` inbound routing into
  Durable Objects (`handleAsNodeConnection`, cloudflare/workerd#7306, #7313) and
  the `node:fs` fixes for positional buffer I/O (#7368), `O_TRUNC` (#7369),
  `O_CREAT` (#7393), and rename over an existing path (#7394).

## Updating a patch

Edit the relevant checkout, then generate a replacement diff against the documented
upstream base from the repository root. Write it under `.work/` to preserve the
commit description at the top of the maintained patch:

```sh
git -C .work/pumpkin add -N crates/pumpkin/src/net/bedrock/nethernet_stub.rs
git -C .work/pumpkin diff b5b9b9d7010e793806a83c495af223c67e1d35ee -- \
  . ':(exclude)Cargo.lock' ':(exclude)crates/pumpkin-world/src/generation' \
  > .work/pumpkin-emscripten.diff
git -C .work/pumpkin diff b5b9b9d7010e793806a83c495af223c67e1d35ee -- \
  crates/pumpkin-world/src/generation > .work/pumpkin-memory.diff
```

Replace the corresponding patch's contents from its first `diff --git` line onward
with the new diff. Update the subject, description, and `Base-commit` when the scope
or pinned revision changes.

The Pumpkin path filters reflect the current separation: all memory-patch files
are under `crates/pumpkin-world/src/generation/`. Adjust the filters if that scope
changes, keeping platform support and memory optimizations in their own patches.

`git diff` omits untracked files, including files created by existing patches;
`add -N` above includes `nethernet_stub.rs`. If you add a file in a dependency
checkout, include it the same way and verify application on a fresh copy of the
base. Run `bash scripts/test.sh` after runtime changes. Keep unpatched checkouts
unmodified. Setup refuses to repin a modified checkout rather than discarding
local work.

For source/patch validation without provisioning the toolchain:

```sh
bash scripts/setup.sh --sources-only
```

## JavaScript dependencies

`package-lock.json` pins Wrangler 4.141.0, workerd 1.20260925.1, `worker-fs-mount` 0.2.0 and
`durable-object-fs` 1.0.0 (the SQLite filesystem mount, imported by
`src/js/mount.js` and bundled by worker-build). `npm install` applies
`patches/worker-fs-mount-open-mode.patch` to the installed `worker-fs-mount`
(numeric open modes masked to their permission bits, as Node does; pending
upstream). worker-build generates
`build/index.js`, which wraps the exports into the entrypoint and derives the
Durable Object class from `DurableObject` for RPC.

After moving a checkout with cached build output, run `cargo clean` before rebuilding.
Generated data can contain absolute paths. This leaves databases under `.data/` intact.
