# Dependency sources and pins

A clean checkout is self-contained after `bash scripts/setup.sh`. Dependency sources
and toolchains are ignored under `.work/`. Both Cargo and npm dependency graphs are
locked.

## Direct checkouts

| Component | Source / branch | Commit | Local changes or purpose |
| --- | --- | --- | --- |
| pumpkin | [master](https://github.com/Pumpkin-MC/Pumpkin) | `b5b9b9d7010e793806a83c495af223c67e1d35ee` | Headless embedding, shared async scheduler, restartable stop signal, compact templates/generation chunks, and build-script fixes |
| tokio | [emscripten-event-loop](https://github.com/guybedford/tokio) | `5315798b6a62a47d03dc40e7d04fbf83c80f3246` | `EventLoopRuntime` (tokio-rs/tokio#8479) and `net` over epoll on Emscripten; unmodified |
| emscripten | [cf-final](https://github.com/guybedford/emscripten) | `4e034c65a18c0034c68e7eb628477fe4428fa1f9` | Upstream main plus emscripten-core/emscripten#27547 (epoll listeners on the host loop), #27698, #27699, and pending fixes: side-module accept, exnref ordering, `fs.constants` in NODEFS; unmodified |
| binaryen | [jspi-hooks](https://github.com/guybedford/binaryen) | `d6483a04d7dab0ef83e5c43f87343061d97a3b8d` | The frontend pin expects this Binaryen; built by setup |
| emsdk | [main](https://github.com/emscripten-core/emsdk) | `5eb0bde7585670252e8ba05e9d361627bffd08b5` | LLVM from emscripten-releases build `e8579ea489b44a6792f5abf95377a6ee38a16cce`, paired with the frontend's main |

Patches are relative to the pinned commits above. Setup checks reverse application
before applying a patch and refuses to repin a modified checkout.

Pumpkin has two patches: `pumpkin-emscripten.patch` for embedding/runtime support,
followed by `pumpkin-memory.patch` for compact templates and generation chunks.
They currently modify separate files and are both based on the pinned upstream
commit.

## Cargo-managed forks

| Crate | Source | Purpose |
| --- | --- | --- |
| mio | https://github.com/guybedford/mio `a62c9e46833fc255c9217ab9aa362c6221ed4401` | Emscripten epoll selector (tokio-rs/mio#1969) |
| libc | https://github.com/rust-lang/libc branch `libc-0.2` | Emscripten epoll bindings, unreleased |
| ring | https://github.com/guybedford/ring branch `emscripten` | getrandom-backed `SystemRandom` on Emscripten |

The root Cargo.toml applies these overrides and the local checkouts. Cargo.lock
pins the full application graph, including the branch commits. The Worker uses
`wasm-bindgen`, `js-sys`, and `web-sys` from crates.io directly; there is no
`worker` crate dependency.

## Host tools

- Rust: `beta` channel (1.99; `OwnedFd::try_clone` on Emscripten), target
  `wasm32-unknown-emscripten`; setup installs both through rustup. rustc needs a
  larger compile-thread stack for pumpkin-data's generated tables; the scripts
  set `RUST_MIN_STACK`.
- wasm-bindgen CLI: the release matching the `wasm-bindgen` crate version in
  Cargo.toml, installed by setup into `.work/bin/`. emcc runs it as a post-link
  step under `-sWASM_BINDGEN`.
- Node: 24+; 26 recommended and selected in CI. Used for build tools and tests.
- Python: 3.11+ (Emscripten scripts and TOML parsing).
- Emscripten backend: LLVM through emsdk under `.work/emsdk` (or an activated
  emsdk selected with `EMSDK` during setup), and Binaryen built from the checkout
  above with CMake and Ninja. Setup writes the frontend's `.emscripten_cf` config
  pointing at both, and `scripts/common.sh` selects it with `EM_CONFIG`.
- workerd: Wrangler must run a workerd with per-Durable-Object port tables,
  `net.Server` inbound routing (`handleAsNodeConnection`), and the `node:fs`
  fixes for positional buffer I/O, `O_TRUNC`, `O_CREAT`, and rename over an
  existing path. Until those ship in Wrangler's bundled version, set
  `MINIFLARE_WORKERD_PATH`.

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

`package-lock.json` pins Wrangler 4.129.0. The only application JavaScript is
`worker/index.mjs`, which re-exports the generated module and derives the
Durable Object class from `DurableObject` for RPC.

After moving a checkout with cached build output, run `cargo clean` before rebuilding.
Generated data can contain absolute paths. This leaves databases under `.data/` intact.
