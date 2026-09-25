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
    # Submodule pointers move with the pin and are not local changes.
    if [ -n "$(git -C "$dir" status --porcelain --ignore-submodules=all)" ]; then
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

checkout_tag() { # name url tag
  local name="$1" url="$2" tag="$3" dir="$WORK/$1"
  if [ ! -d "$dir/.git" ] && [ ! -f "$dir/.git" ]; then
    if [ -e "$dir" ]; then
      echo "error: $dir exists but is not a Git checkout." >&2
      exit 1
    fi
    git clone --filter=blob:none --branch "$tag" --single-branch "$url" "$dir"
  fi
  if ! git -C "$dir" cat-file -e "refs/tags/$tag^{commit}" 2>/dev/null; then
    git -C "$dir" fetch --filter=blob:none origin "tag" "$tag" --no-tags
  fi
  if [ "$(git -C "$dir" rev-parse HEAD)" != "$(git -C "$dir" rev-parse "refs/tags/$tag^{commit}")" ]; then
    # Submodule pointers move with the pin and are not local changes.
    if [ -n "$(git -C "$dir" status --porcelain --ignore-submodules=all)" ]; then
      echo "error: refusing to repin modified checkout $dir; preserve your changes first." >&2
      exit 1
    fi
    git -C "$dir" switch --detach "refs/tags/$tag"
  fi
  echo "  $name @ $tag ($(git -C "$dir" rev-parse --short HEAD))"
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
apply_patch pumpkin pumpkin-emscripten.patch
apply_patch pumpkin pumpkin-memory.patch

if [ "${1:-}" = --sources-only ]; then exit 0; fi

echo "==> Rust toolchain"
RUST_CHANNEL="$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["toolchain"]["channel"])' "$REPO/rust-toolchain.toml")"
rustup toolchain install "$RUST_CHANNEL" --profile minimal --target wasm32-unknown-emscripten --no-self-update

echo "==> worker-build"
# The release matching the `worker` crate version in Cargo.toml. Built for the
# host, overriding the wasm target .cargo/config.toml sets.
WORKER_VERSION="$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["dependencies"]["worker"]["version"])' "$REPO/Cargo.toml")"
if [ "$("$BIN/worker-build" --version 2>/dev/null || true)" != "worker-build $WORKER_VERSION" ]; then
  HOST="$(rustc "+$RUST_CHANNEL" -vV | sed -n 's/^host: //p')"
  cargo "+$RUST_CHANNEL" install --force --locked --target "$HOST" worker-build --version "$WORKER_VERSION" --root "$WORK"
fi

echo "==> Worker dependencies"
(cd "$REPO" && "$NODE" "$(dirname "$("$NODE" -p 'process.execPath')")/npm" ci --no-audit --no-fund)

echo "Setup complete. Run: bash scripts/test.sh  or  bash scripts/serve.sh"
