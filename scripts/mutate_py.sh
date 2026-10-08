#!/usr/bin/env bash
# Mutation-test the apworld with mutmut: `run [glob]` (default), or any mutmut command (`results`, `show <name>`, `browse`).
# mutmut names mutants after the file path, but the tests import the world as `worlds.ap_go2`, so the code is staged in
# .mutate-py/worlds/ap_go2 and .ap/worlds/ap_go2 points at mutmut's mutated copy for the run (restored on exit).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
[ -d "$root/.ap/.git" ] || { echo "Run 'just setup-ap' first." >&2; exit 1; }
work="$root/.mutate-py"
link="$root/.ap/worlds/ap_go2"

# Refresh the staged sources; keep mutants/ so mutmut reuses results for unchanged code.
mkdir -p "$work/worlds"
rm -rf "$work/worlds/ap_go2" "$work/tests" "$work/docs"
cp -r "$root/apworld/ap_go2" "$work/worlds/ap_go2"
cp -r "$root/apworld/tests" "$root/apworld/docs" "$work/"
cat > "$work/pyproject.toml" <<'TOML'
[tool.mutmut]
paths_to_mutate = ["worlds/ap_go2/"]
tests_dir = ["tests/"]
also_copy = ["docs/"] # the slot_data schema and sample the tests read
pytest_add_cli_args = ["--ignore=tests/test_build.py"] # packaging test: shells out to scripts/ outside the copy
TOML

trap 'ln -sfn "$root/apworld/ap_go2" "$link"' EXIT
ln -sfn "$work/mutants/worlds/ap_go2" "$link"
cd "$work"
PYTHONPATH="$root/.ap" "$root/apworld/.venv/bin/mutmut" "${@:-run}"
