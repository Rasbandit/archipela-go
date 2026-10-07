#!/usr/bin/env bash
# Clone the pinned Archipelago into .ap/ and link our world into it for WorldTestBase.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
ref="$(tr -d '[:space:]' < "$root/.ap-version")"

if [ ! -d "$root/.ap/.git" ]; then
  git clone --depth 1 --branch "$ref" https://github.com/ArchipelagoMW/Archipelago.git "$root/.ap"
fi
ln -sfn "$root/apworld/ap_go2" "$root/.ap/worlds/ap_go2"
uv sync --project "$root/apworld"
uv pip install --python "$root/apworld/.venv/bin/python" -r "$root/.ap/requirements.txt"
echo "Archipelago $ref ready in .ap/"
