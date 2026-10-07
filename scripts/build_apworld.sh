#!/usr/bin/env bash
# Build dist/ap_go2.apworld with Archipelago's own "Build APWorlds" component, so the manifest gets the
# `version` and `compatible_version` fields that Archipelago 0.7.0 requires.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
[ -d "$root/.ap/.git" ] || { echo "Run 'just setup-ap' first." >&2; exit 1; }

cd "$root/.ap"
rm -rf build/apworlds
PYTHONPATH=. "$root/apworld/.venv/bin/python" - <<'PY'
import json
import runpy
import sys

import ModuleUpdate

ModuleUpdate.update_ran = True
with open("worlds/ap_go2/archipelago.json", encoding="utf-8") as f:
    game = json.load(f)["game"]
sys.argv = ["Launcher.py", "Build APWorlds", "--", game, "--skip_open_folder"]
runpy.run_path("Launcher.py", run_name="__main__")
PY

mkdir -p "$root/dist"
cp build/apworlds/ap_go2.apworld "$root/dist/ap_go2.apworld"
echo "built dist/ap_go2.apworld"
