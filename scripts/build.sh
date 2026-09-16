#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
require_node
require_toolchain
cd "$REPO"
if [ "$(uname -s)" = Darwin ]; then
  # Binaryen's worker threads overflow their stack optimizing this module on
  # macOS; run wasm-opt on the main thread with the largest stack allowed.
  ulimit -s hard
  export BINARYEN_CORES=1
fi
worker-build --emscripten --release -- --locked
