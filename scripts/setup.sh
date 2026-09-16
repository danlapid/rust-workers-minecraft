#!/usr/bin/env bash
# Clone pinned sources, apply dependency patches, and provision the toolchain.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != --sources-only ]; }; then
  echo "usage: bash scripts/setup.sh [--sources-only]" >&2
  exit 2
fi
require_node
mkdir -p "$WORK"

checkout() { # name url branch commit
  local name="$1" url="$2" branch="$3" commit="$4" dir="$WORK/$1"
  if [ ! -d "$dir/.git" ] && [ ! -f "$dir/.git" ]; then
    if [ -e "$dir" ]; then
      echo "error: $dir exists but is not a Git checkout." >&2
      exit 1
    fi
    git clone --filter=blob:none --branch "$branch" --single-branch "$url" "$dir"
  fi
  if [ "$(git -C "$dir" rev-parse HEAD)" != "$commit" ]; then
    if [ -n "$(git -C "$dir" status --porcelain)" ]; then
      echo "error: refusing to repin modified checkout $dir; preserve your changes first." >&2
      exit 1
    fi
    if ! git -C "$dir" cat-file -e "$commit^{commit}" 2>/dev/null; then
      git -C "$dir" fetch --filter=blob:none origin "$branch"
    fi
    git -C "$dir" switch --detach "$commit"
  fi
  echo "  $name @ $commit"
}

apply_patch() {
  local dir="$WORK/$1" patch="$REPO/patches/$2"
  if git -C "$dir" apply --reverse --check "$patch" 2>/dev/null; then
    echo "  already applied: $2"
  else
    git -C "$dir" apply --check "$patch"
    git -C "$dir" apply "$patch"
  fi
}

echo "==> Pinned dependencies"
checkout pumpkin https://github.com/Pumpkin-MC/Pumpkin master b5b9b9d7010e793806a83c495af223c67e1d35ee
checkout tokio https://github.com/guybedford/tokio emscripten-event-loop 5315798b6a62a47d03dc40e7d04fbf83c80f3246
apply_patch pumpkin pumpkin-emscripten.patch
apply_patch pumpkin pumpkin-memory.patch

if [ "${1:-}" = --sources-only ]; then exit 0; fi

echo "==> Rust toolchain"
RUST_CHANNEL="$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["toolchain"]["channel"])' "$REPO/rust-toolchain.toml")"
rustup toolchain install "$RUST_CHANNEL" --profile minimal --target wasm32-unknown-emscripten --no-self-update

echo "==> wasm-bindgen CLI"
WASM_BINDGEN_VERSION="$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["dependencies"]["wasm-bindgen"])' "$REPO/Cargo.toml")"
if [ "$("$WASM_BINDGEN_BIN/wasm-bindgen" --version 2>/dev/null || true)" != "wasm-bindgen $WASM_BINDGEN_VERSION" ]; then
  cargo "+$RUST_CHANNEL" install --force --locked wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --root "$(dirname "$WASM_BINDGEN_BIN")"
fi

echo "==> Emscripten"
# Frontend: upstream main plus the pending JSPI hooks, reentrant JSPI, and epoll
# listener PRs. LLVM comes from the emscripten-releases build paired with that
# main, and Binaryen from the branch carrying the jspi-hooks pass the frontend
# needs; setup builds it. EMSDK selects an activated emsdk for LLVM instead.
EMSDK_RELEASE=e8579ea489b44a6792f5abf95377a6ee38a16cce
checkout emscripten https://github.com/guybedford/emscripten cf-final 4e034c65a18c0034c68e7eb628477fe4428fa1f9
NODE_PATH="$("$NODE" -p 'process.execPath')"
(cd "$EMSCRIPTEN" && PATH="$(dirname "$NODE_PATH"):$PATH" npm ci --no-audit --no-fund && python3 bootstrap.py)
if [ -n "${EMSDK:-}" ]; then
  LLVM_ROOT="$EMSDK/upstream/bin"
else
  checkout emsdk https://github.com/emscripten-core/emsdk main 5eb0bde7585670252e8ba05e9d361627bffd08b5
  if [ "$(cat "$WORK/emsdk/upstream/.emsdk_version" 2>/dev/null)" != "releases-$EMSDK_RELEASE-64bit" ]; then
    (cd "$WORK/emsdk" && ./emsdk install "$EMSDK_RELEASE" && ./emsdk activate "$EMSDK_RELEASE")
  fi
  LLVM_ROOT="$WORK/emsdk/upstream/bin"
fi
checkout binaryen https://github.com/guybedford/binaryen jspi-hooks d6483a04d7dab0ef83e5c43f87343061d97a3b8d
BINARYEN_ROOT="$WORK/binaryen/build"
if [ ! -x "$BINARYEN_ROOT/bin/wasm-opt" ] || [ "$(cat "$BINARYEN_ROOT/.pinned" 2>/dev/null)" != "$(git -C "$WORK/binaryen" rev-parse HEAD)" ]; then
  cmake -S "$WORK/binaryen" -B "$BINARYEN_ROOT" -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTS=OFF >/dev/null
  cmake --build "$BINARYEN_ROOT" --target wasm-opt wasm-metadce wasm-emscripten-finalize wasm-split wasm-as wasm-dis wasm2js wasm-merge >/dev/null
  git -C "$WORK/binaryen" rev-parse HEAD > "$BINARYEN_ROOT/.pinned"
fi
if [ ! -x "$LLVM_ROOT/clang" ] || [ ! -x "$BINARYEN_ROOT/bin/wasm-opt" ]; then
  echo "error: no Emscripten backend at LLVM_ROOT=$LLVM_ROOT BINARYEN_ROOT=$BINARYEN_ROOT." >&2
  exit 1
fi
python3 - "$EMSCRIPTEN/.emscripten_cf" "$LLVM_ROOT" "$BINARYEN_ROOT" "$NODE_PATH" <<'PY'
from pathlib import Path
import sys
config, llvm, binaryen, node = sys.argv[1:]
Path(config).write_text(
    f"LLVM_ROOT={llvm!r}\n"
    f"BINARYEN_ROOT={binaryen!r}\n"
    f"NODE_JS={node!r}\n"
)
PY
echo "Setup complete. Run: bash scripts/test.sh  or  bash scripts/serve.sh"
