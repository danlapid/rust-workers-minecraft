# Dependency sources and pins

A clean checkout is self-contained after `bash scripts/setup.sh`. Dependency sources
and toolchains are ignored under `.work/`. Both Cargo and npm dependency graphs are
locked.

## Direct checkouts

| Component | Source / branch | Commit | Local changes or purpose |
| --- | --- | --- | --- |
| pumpkin | [master](https://github.com/Pumpkin-MC/Pumpkin) | `b5b9b9d7010e793806a83c495af223c67e1d35ee` | Headless embedding, shared async scheduler, compact templates/generation chunks, and build-script fixes |
| tokio | [emscripten-epoll](https://github.com/guybedford/tokio) | `8d0a2a845c546e93a4cf0e8ff2c21ac50a3d1931` | JSPI parking and `net` over epoll on Emscripten (tokio-rs/tokio#8281 follow-ons); unmodified |
| emscripten | [cf-final](https://github.com/guybedford/emscripten) | `a5013dd597ec9d48856370f5707d438937a7b4e8` | Upstream main plus emscripten-core/emscripten#27698, #27699 (`REENTRANT_JSPI`), #27547, and two fixes pending for #27699; unmodified |
| binaryen | [jspi-hooks](https://github.com/guybedford/binaryen) | `d6483a04d7dab0ef83e5c43f87343061d97a3b8d` | WebAssembly/binaryen#9102, the `--jspi-hooks` pass; built by setup |
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
pins the full application graph, including the branch commits.

## Host tools

- Rust: `beta` channel (1.99; `OwnedFd::try_clone` on Emscripten), target
  `wasm32-unknown-emscripten`; setup installs both through rustup. rustc needs a
  larger compile-thread stack for pumpkin-data's generated tables; the scripts
  set `RUST_MIN_STACK`.
- wasm-bindgen CLI: the release matching the `wasm-bindgen` crate version in
  Cargo.toml, installed by setup into `.work/bin/` (or `WASM_BINDGEN_BIN`). emcc
  runs it as a post-link step under `-sWASM_BINDGEN`.
- Node: 24+; 26 recommended and selected in CI. Used for build tools and tests.
- Python: 3.11+ (Emscripten scripts and TOML parsing).
- Emscripten backend: LLVM through emsdk under `.work/emsdk` (or an activated
  emsdk selected with `EMSDK`), and Binaryen built from the checkout above with
  CMake and Ninja. Setup writes the frontend's machine-local `.emscripten_cf`
  pointing at both. Homebrew's release cannot be used while the frontend is
  ahead of the tagged releases.
- workerd: Wrangler must run a workerd with per-Durable-Object port tables and
  `net.Server` inbound routing (`handleAsNodeConnection`). Until that ships in
  Wrangler's bundled version, set `MINIFLARE_WORKERD_PATH`.

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

`package-lock.json` pins Wrangler 4.129.0, worker-fs-mount 0.2.0,
durable-object-fs 1.0.0, TypeScript, and their dependencies. The npm override for
durable-object-fs replaces its published `workspace:*` peer dependency with the
selected worker-fs-mount version. Generated runtime/binding types are recreated by
`npm run types` and are not committed.

After moving a checkout with cached build output, run `cargo clean` before rebuilding.
Generated data can contain absolute paths. This leaves databases under `.data/` intact.
