#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$REPO/.work"
NODE="${NODE:-node}"
# Setup provisions the frontend checkout, compiler backend config, and the
# wasm-bindgen CLI under .work/; WASM_BINDGEN_BIN may point at another CLI directory.
EMSCRIPTEN="$WORK/emscripten"
WASM_BINDGEN_BIN="${WASM_BINDGEN_BIN:-$WORK/bin}"

require_node() {
  local major
  major="$("$NODE" -p 'process.versions.node.split(".")[0]')"
  if [ "$major" -lt 24 ]; then
    echo "error: Node >= 24 required. Select it or set NODE=/path/to/node." >&2
    exit 1
  fi
}

require_toolchain() {
  if [ ! -f "$EMSCRIPTEN/.emscripten_cf" ] ||
     [ ! -x "$EMSCRIPTEN/emcc" ] ||
     [ ! -x "$WASM_BINDGEN_BIN/wasm-bindgen" ] ||
     [ ! -f "$WORK/pumpkin/crates/pumpkin/Cargo.toml" ] ||
     [ ! -f "$WORK/tokio/tokio/Cargo.toml" ]; then
    echo "error: run bash scripts/setup.sh first." >&2
    exit 1
  fi
  export EM_CONFIG="$EMSCRIPTEN/.emscripten_cf"
  export PATH="$EMSCRIPTEN:$WASM_BINDGEN_BIN:$PATH"
  export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER="$EMSCRIPTEN/emcc"
  # Uniform exnref exception handling for C and Rust objects (see .cargo/config.toml).
  export EMCC_CFLAGS="${EMCC_CFLAGS:-} -fwasm-exceptions -sWASM_LEGACY_EXCEPTIONS=0"
  export CARGO_TARGET_DIR="$REPO/target/workers"
  # rustc's LLVM const emission recurses deeply on pumpkin-data's generated tables
  # and overflows its default 8 MiB compile-thread stack on current toolchains.
  export RUST_MIN_STACK="${RUST_MIN_STACK:-268435456}"
}

require_packages() {
  if [ ! -f "$REPO/node_modules/wrangler/bin/wrangler.js" ]; then
    echo "error: run npm ci first." >&2
    exit 1
  fi
}
