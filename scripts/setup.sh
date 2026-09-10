#!/usr/bin/env bash
# Clone pinned sources, apply dependency patches, and build the host toolchain.
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
  if [ ! -d "$dir/.git" ]; then
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
checkout emscripten https://github.com/guybedford/emscripten cf 21166256c4c4d73d39b3685c8973d7cbe427ce8c
checkout wasm-bindgen https://github.com/guybedford/wasm-bindgen emscripten-non-identifier-names 4b69f3b3ba4212c857be6854f77fa5aec8b62871
checkout tokio https://github.com/guybedford/tokio emscripten-layering 7c1d4977c510866775ed6164b58b2218a6a2955b
checkout libc https://github.com/guybedford/libc libc-0.2-emscripten 4091fe0b0dc5f9c1a27bed75be1ff02bb27e756d
checkout ring https://github.com/guybedford/ring emscripten 6671f7cfbb13f249b571ffa6326275a8596e0ca2
checkout pumpkin https://github.com/Pumpkin-MC/Pumpkin master b5b9b9d7010e793806a83c495af223c67e1d35ee
checkout workers-rs https://github.com/ThomasRubini/workers-rs connect-bindings 7db011ec97658a5d907f3e3102028ce86c044f19
apply_patch wasm-bindgen wasm-bindgen-emscripten-closures.patch
apply_patch pumpkin pumpkin-emscripten.patch
apply_patch pumpkin pumpkin-memory.patch
apply_patch workers-rs workers-rs-emscripten-toolchain.patch

if [ "${1:-}" = --sources-only ]; then exit 0; fi

echo "==> Pinned Rust toolchain"
RUST_CHANNEL="$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["toolchain"]["channel"])' "$REPO/rust-toolchain.toml")"
rustup toolchain install "$RUST_CHANNEL" --profile minimal --target wasm32-unknown-emscripten --no-self-update

echo "==> Emscripten frontend and Homebrew compiler backend"
EM_PREFIX="$(brew --prefix emscripten)" || {
  echo "error: install the backend with 'brew install emscripten'." >&2
  exit 1
}
# Resolve NODE before entering the checkout (it may be a relative path).
NODE_PATH="$("$NODE" -p 'process.execPath')"
export PATH="$(dirname "$NODE_PATH"):$PATH"
(cd "$WORK/emscripten" && npm ci --no-audit --no-fund && python3 bootstrap.py)
python3 - "$WORK/emscripten/.emscripten_cf" "$EM_PREFIX" "$NODE_PATH" <<'PY'
from pathlib import Path
import sys
config, prefix, node = sys.argv[1:]
Path(config).write_text(
    f"LLVM_ROOT={prefix + '/libexec/llvm/bin'!r}\n"
    f"BINARYEN_ROOT={prefix + '/libexec/binaryen'!r}\n"
    f"NODE_JS={node!r}\n"
)
PY

echo "==> Patched wasm-bindgen CLI"
# Upstream does not commit this workspace lockfile. Preserve the proven CLI resolution.
cp "$REPO/toolchain/wasm-bindgen.Cargo.lock" "$WORK/wasm-bindgen/Cargo.lock"
# Override nested dependency toolchain files with the repository's dated nightly.
HOST_TARGET="$(rustc "+$RUST_CHANNEL" -vV | sed -n 's/^host: //p')"
(cd "$WORK/wasm-bindgen" && cargo "+$RUST_CHANNEL" build --locked --release -p wasm-bindgen-cli \
  --target "$HOST_TARGET" --target-dir "$WORK/wasm-bindgen/target")
mkdir -p "$WORK/bin"
cp "$WORK/wasm-bindgen/target/$HOST_TARGET/release/wasm-bindgen" "$WORK/bin/wasm-bindgen"
echo "Setup complete. Run: bash scripts/test.sh  or  bash scripts/serve.sh"
