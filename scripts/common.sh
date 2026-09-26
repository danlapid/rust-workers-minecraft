#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$REPO/.work"
NODE="${NODE:-node}"
# Setup provisions the patched Pumpkin checkout and the worker-build CLI under
# .work/; worker-build provisions Emscripten and the wasm-bindgen CLI itself.
BIN="$WORK/bin"

require_node() {
  local major
  major="$("$NODE" -p 'process.versions.node.split(".")[0]')"
  if [ "$major" -lt 24 ]; then
    echo "error: Node >= 24 required. Select it or set NODE=/path/to/node." >&2
    exit 1
  fi
}

require_toolchain() {
  if [ ! -x "$BIN/worker-build" ] ||
     [ ! -f "$WORK/pumpkin/crates/pumpkin/Cargo.toml" ]; then
    echo "error: run bash scripts/setup.sh first." >&2
    exit 1
  fi
  export PATH="$BIN:$PATH"
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
