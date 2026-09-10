# Dependency sources and pins

A clean checkout is self-contained after `bash scripts/setup.sh`. Dependency sources
are ignored under `.work/`. Both Cargo and npm dependency graphs are locked.

## Direct checkouts

| Component | Source / branch | Commit | Local changes or purpose |
| --- | --- | --- | --- |
| emscripten | [cf](https://github.com/guybedford/emscripten) | `21166256c4c4d73d39b3685c8973d7cbe427ce8c` | Fork frontend: epoll, JSPI, wasm-bindgen post-link |
| wasm-bindgen | [emscripten-non-identifier-names](https://github.com/guybedford/wasm-bindgen) | `4b69f3b3ba4212c857be6854f77fa5aec8b62871` | Closure-finalizer string escaping |
| tokio | [emscripten-layering](https://github.com/guybedford/tokio) | `7c1d4977c510866775ed6164b58b2218a6a2955b` | HostedRuntime and normal Tokio TCP/UDP APIs; unmodified |
| libc | [libc-0.2-emscripten](https://github.com/guybedford/libc) | `4091fe0b0dc5f9c1a27bed75be1ff02bb27e756d` | Emscripten epoll and socket support; unmodified |
| ring | [emscripten](https://github.com/guybedford/ring) | `6671f7cfbb13f249b571ffa6326275a8596e0ca2` | Portable Emscripten crypto backend; unmodified |
| pumpkin | [master](https://github.com/Pumpkin-MC/Pumpkin) | `b5b9b9d7010e793806a83c495af223c67e1d35ee` | Headless embedding, shared async scheduler, compact templates/generation chunks, and build-script fixes |
| workers-rs | [connect-bindings](https://github.com/ThomasRubini/workers-rs) | `7db011ec97658a5d907f3e3102028ce86c044f19` | TCP ingress PR #1041; pinned ABI, Emscripten Tokio promise adapter, host-owned initialization, and socket shutdown flushing |

Patches are relative to the pinned commits above. Setup checks reverse application
before applying a patch and refuses to repin a modified checkout.

Pumpkin has two patches: `pumpkin-emscripten.patch` for embedding/runtime support,
followed by `pumpkin-memory.patch` for compact templates and generation chunks.
They currently modify separate files and are both based on the pinned upstream
commit.

## Cargo-managed forks

| Crate | Source | Commit |
| --- | --- | --- |
| mio | https://github.com/guybedford/mio | `a62c9e46833fc255c9217ab9aa362c6221ed4401` |
| wasm-streams | https://github.com/guybedford/wasm-streams | `115f0f27380a4f5fa33cbd9427a7107ec94cb557` |

The root Cargo.toml applies these overrides and the local checkouts. Cargo.lock
pins the full application graph. `toolchain/wasm-bindgen.Cargo.lock` separately pins
the host CLI graph, since that upstream workspace does not commit a lockfile.

## Host tools

- Rust: `nightly-2026-07-20`, target `wasm32-unknown-emscripten`; setup installs both through rustup.
- Node: 24+; 26 recommended and selected in CI. Used for build tools and tests.
- Python: 3.11+ (Emscripten scripts and TOML parsing).
- Backend: Homebrew `emscripten`, detected with `brew --prefix emscripten`.
  Homebrew backend versions are external prerequisites, not pinned source files.

Setup deliberately uses the fork frontend with Homebrew's LLVM/binaryen backend;
emsdk is not needed. It writes a machine-local `.emscripten_cf`, builds wasm-bindgen
explicitly for the host target, and places the CLI in `.work/bin/`.

## Updating a patch

Edit the relevant checkout, then generate a replacement diff against the documented
upstream base from the repository root. Write it under `.work/` to preserve the
commit description at the top of the maintained patch:

```sh
git -C .work/pumpkin diff b5b9b9d7010e793806a83c495af223c67e1d35ee -- \
  . ':(exclude)Cargo.lock' ':(exclude)crates/pumpkin-world/src/generation' \
  > .work/pumpkin-emscripten.diff
git -C .work/pumpkin diff b5b9b9d7010e793806a83c495af223c67e1d35ee -- \
  crates/pumpkin-world/src/generation > .work/pumpkin-memory.diff
git -C .work/wasm-bindgen diff 4b69f3b3ba4212c857be6854f77fa5aec8b62871 -- crates/cli-support/src/js/mod.rs > .work/wasm-bindgen-emscripten-closures.diff
git -C .work/workers-rs diff 7db011ec97658a5d907f3e3102028ce86c044f19 > .work/workers-rs-emscripten-toolchain.diff
```

Replace the corresponding patch's contents from its first `diff --git` line onward
with the new diff. Update the subject, description, and `Base-commit` when the scope
or pinned revision changes.

The Pumpkin path filters reflect the current separation: all memory-patch files
are under `crates/pumpkin-world/src/generation/`. Adjust the filters if that scope
changes, keeping platform support and memory optimizations in their own patches.

`git diff` omits untracked files, including files created by existing patches.
Preserve those added-file diffs when regenerating (for example,
`nethernet_stub.rs` in the compatibility patch). If you add a file in a dependency
checkout, include it explicitly and verify application on a fresh copy of the base.
Run `bash scripts/test.sh` after runtime changes. Keep unpatched checkouts unmodified.
Setup refuses to repin a modified checkout rather than discarding local work.

For source/patch validation without rebuilding the toolchain:

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
