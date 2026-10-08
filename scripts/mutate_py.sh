#!/usr/bin/env bash
# Mutation-test the apworld with mutmut: `run [glob]` (default), or any mutmut command (`results`, `show <name>`, `browse`).
# mutmut names mutants after the file path, but the tests import the world as `worlds.ap_go2`, so the code is staged in
# .mutate-py/worlds/ap_go2 and, for `run` only, .ap/worlds/ap_go2 points at mutmut's mutated copy (restored on exit; if the
# run is killed, `just check-py` refuses to run until `just setup-ap` puts the link back).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
[ -d "$root/.ap/.git" ] || { echo "Run 'just setup-ap' first." >&2; exit 1; }
work="$root/.mutate-py"
link="$root/.ap/worlds/ap_go2"
mutmut=(uv run --project "$root/apworld" mutmut)
[ $# -gt 0 ] || set -- run

mkdir -p "$work/worlds"
if [ "$1" != run ]; then
  cd "$work"
  exec env PYTHONPATH="$root/.ap" "${mutmut[@]}" "$@"
fi

# One run at a time: the .ap link is shared, and a second run would put it back under the first.
exec 9> "$work/.lock"
flock -n 9 || { echo "Another 'just mutate-py run' is in progress." >&2; exit 1; }

# Refresh the staged sources; keep mutants/ so mutmut reuses results for unchanged code.
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
PYTHONPATH="$root/.ap" "${mutmut[@]}" "$@"
