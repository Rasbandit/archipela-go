#!/usr/bin/env bash
# Dev Archipelago server with OUR apworld, single player "Tester".
#   [APGO_ZONES=walk,bike,drive] [APGO_GOAL=macguffin_short] scripts/ap_host.sh start [trips]
#                                      generate a seed and host it on :38281 (+ adb reverse to the phone)
#   scripts/ap_host.sh stop | status | log
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dev="$root/.ap/dev"
py="$root/apworld/.venv/bin/python"
port=38281
game="Archipela-Go 2: Electric Boogaloo"

run_ap() {  # run an Archipelago entry script with its self-updater disabled
  (cd "$root/.ap" && PYTHONPATH=. "$py" - "$@" <<'PY'
import runpy, sys
import ModuleUpdate
ModuleUpdate.update_ran = True
script = sys.argv[1]
sys.argv = sys.argv[1:]
runpy.run_path(script, run_name="__main__")
PY
  )
}

case "${1:-}" in
  start)
    trips="${2:-100}"
    "$0" stop >/dev/null 2>&1 || true
    rm -rf "$dev"; mkdir -p "$dev/players" "$dev/out"
    zones="${APGO_ZONES:-walk,bike,drive}"
    goal="${APGO_GOAL:-macguffin_short}"
    cat > "$dev/players/Tester.yaml" <<YAML
name: Tester
game: "$game"
"$game":
  goal: $goal
  number_of_trips: $trips
  zone_modes: [${zones//,/, }]
  easy_share: 50
  medium_share: 35
  hard_share: 15
YAML
    run_ap Generate.py --player_files_path "$dev/players" --outputpath "$dev/out" --seed 7 >"$dev/generate.log" 2>&1 \
      || { tail -20 "$dev/generate.log"; exit 1; }
    (cd "$dev/out" && unzip -q -o AP_*.zip '*.archipelago' && ls *.archipelago >/dev/null)
    data="$(ls "$dev"/out/*.archipelago | head -1)"
    nohup bash -c "cd '$root/.ap' && PYTHONPATH=. '$py' - MultiServer.py '$data' --host 0.0.0.0 --port $port --savefile '$dev/save' <<'PY'
import runpy, sys
import ModuleUpdate
ModuleUpdate.update_ran = True
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name='__main__')
PY" >"$dev/server.log" 2>&1 &
    echo $! > "$dev/server.pid"
    sleep 4
    adb reverse tcp:$port tcp:$port >/dev/null 2>&1 && echo "adb reverse set: phone localhost:$port -> this machine" || echo "(no phone for adb reverse)"
    echo "server up (pid $(cat "$dev/server.pid")), slot Tester, $trips trips, zones $zones, goal $goal. Connect to localhost:$port"
    ;;
  stop)
    [ -f "$dev/server.pid" ] && kill "$(cat "$dev/server.pid")" 2>/dev/null || true
    for pid in $(ss -ltnpH "sport = :$port" 2>/dev/null | grep -oE 'pid=[0-9]+' | cut -d= -f2 | sort -u); do kill "$pid" 2>/dev/null || true; done
    sleep 1
    echo stopped
    ;;
  status) ss -ltnH "sport = :$port" | grep -q . && echo "running on :$port" || echo "not running" ;;
  log) tail -n "${2:-40}" "$dev/server.log" ;;
  *) echo "usage: $0 start [trips] | stop | status | log"; exit 2 ;;
esac
