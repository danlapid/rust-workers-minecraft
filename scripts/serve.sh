#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
require_node
require_packages
"$NODE" "$REPO/scripts/check-ports.mjs" 25565 8787
bash "$REPO/scripts/build.sh"
cd "$REPO"
exec "$NODE" node_modules/wrangler/bin/wrangler.js dev --local --persist-to "$REPO/.data/workers/server"
