#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$REPO/.work"
NODE="${NODE:-node}"

require_node() {
  local major
  major="$("$NODE" -p 'process.versions.node.split(".")[0]')"
  if [ "$major" -lt 24 ]; then
    echo "error: Node >= 24 required. Select it or set NODE=/path/to/node." >&2
    exit 1
  fi
}

require_toolchain() {
  if [ ! -f "$WORK/emscripten/.emscripten_cf" ] ||
     [ ! -x "$WORK/bin/wasm-bindgen" ] ||
     [ ! -x "$WORK/emsdk/upstream/bin/clang" ] ||
     [ ! -x "$WORK/emsdk/upstream/bin/wasm-opt" ] ||
     [ ! -f "$WORK/tokio-compat/tokio/Cargo.toml" ] ||
     [ ! -f "$WORK/ring-compat/Cargo.toml" ] ||
     [ ! -f "$WORK/proc-macro-error2/Cargo.toml" ] ||
     [ ! -f "$WORK/pumpkin/crates/pumpkin/Cargo.toml" ] ||
     [ ! -f "$WORK/workers-rs/worker/Cargo.toml" ]; then
    echo "error: run bash scripts/setup.sh first." >&2
    exit 1
  fi
  export EM_CONFIG="$WORK/emscripten/.emscripten_cf"
  export PATH="$WORK/emscripten:$WORK/bin:$PATH"
  export CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER="$WORK/emscripten/emcc"
  export CARGO_TARGET_DIR="$REPO/target/workers"
}

require_packages() {
  if [ ! -f "$REPO/node_modules/wrangler/bin/wrangler.js" ]; then
    echo "error: run npm ci first." >&2
    exit 1
  fi
}
