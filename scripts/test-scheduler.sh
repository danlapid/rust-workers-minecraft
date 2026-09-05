#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cd "$REPO"
TEST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
CARGO_TARGET_DIR="$REPO/target/native-tests" cargo test \
  --manifest-path .work/pumpkin/Cargo.toml --target "$TEST_TARGET" \
  -p pumpkin-world --no-default-features --features single-threaded --lib storage_tests
