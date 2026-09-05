#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
require_node
require_toolchain
cd "$REPO"
cargo build --locked --release --bin pumpkin-do
