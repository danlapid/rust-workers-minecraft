#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
require_node
require_packages
require_toolchain
"$NODE" "$REPO/scripts/check-ports.mjs" 25565 8787
bash "$REPO/scripts/build.sh"
cd "$REPO"
exec "$NODE" tests/integration.mjs
