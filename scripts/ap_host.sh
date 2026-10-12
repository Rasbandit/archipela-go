#!/usr/bin/env bash
# Dev Archipelago server with OUR apworld, single player "Tester".
#   [APGO_ZONES=walk,bike,drive] [APGO_GOALS="Letter Hunt,The Big One"] [APGO_REQ=require_all_goals] scripts/ap_host.sh start [trips]
#                                      generate a seed and host it on :38281 (+ adb reverse to the phone)
#   scripts/ap_host.sh serve           host the last generated seed again on its save (a server restart; nothing regenerated)
#   scripts/ap_host.sh stop | status | log
# APGO_AP_DIR and APGO_AP_PORT move it (the compliance tests use their own); APGO_AP_NO_ADB=1 skips adb reverse;
# APGO_AP_SERVER_ARGS adds MultiServer options (the compliance tests turn release and collect off).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dev="${APGO_AP_DIR:-$root/.ap/dev}"
py="$root/apworld/.venv/bin/python"
port="${APGO_AP_PORT:-38281}"
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

# Host the generated seed on its save in the background and wait until the port listens (at most 30 s).
serve() {
  data="$(find "$dev/out" -maxdepth 1 -name '*.archipelago' | sort | head -1)"
  [ -n "$data" ] || { echo "no generated seed in $dev/out: run start first"; exit 1; }
  nohup bash -c "cd '$root/.ap' && PYTHONPATH=. '$py' - MultiServer.py '$data' --host 0.0.0.0 --port $port --savefile '$dev/save' ${APGO_AP_SERVER_ARGS:-} <<'PY'
import runpy, sys
import ModuleUpdate
ModuleUpdate.update_ran = True
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name='__main__')
PY" >>"$dev/server.log" 2>&1 &
  echo $! > "$dev/server.pid"
  for _ in $(seq 60); do ss -ltnH "sport = :$port" | grep -q . && return 0; sleep 0.5; done
  echo "server did not listen on :$port"; tail -20 "$dev/server.log"; exit 1
}

case "${1:-}" in
  start)
    trips="${2:-100}"
    "$0" stop >/dev/null 2>&1 || true
    rm -rf "$dev"; mkdir -p "$dev/players" "$dev/out"
    zones="${APGO_ZONES:-walk,bike,drive}"
    goals="${APGO_GOALS:-Letter Hunt}"
    req="${APGO_REQ:-require_any_one_goal}"
    cat > "$dev/players/Tester.yaml" <<YAML
name: Tester
game: "$game"
"$game":
  goal_selection: [${goals}]
  goal_requirement: $req
  goals_required: ${APGO_NEED:-2}
  number_of_trips: $trips
  zone_modes: [${zones//,/, }]
  easy_share: 50
  medium_share: 35
  hard_share: 15
YAML
    run_ap Generate.py --player_files_path "$dev/players" --outputpath "$dev/out" --seed 7 >"$dev/generate.log" 2>&1 \
      || { tail -20 "$dev/generate.log"; exit 1; }
    (cd "$dev/out" && unzip -q -o AP_*.zip '*.archipelago' && ls -- *.archipelago >/dev/null)
    serve
    [ -n "${APGO_AP_NO_ADB:-}" ] || { adb reverse tcp:"$port" tcp:"$port" >/dev/null 2>&1 && echo "adb reverse set: phone localhost:$port -> this machine" || echo "(no phone for adb reverse)"; }
    echo "server up (pid $(cat "$dev/server.pid")), slot Tester, $trips trips, zones $zones, goals $goals ($req). Connect to localhost:$port"
    ;;
  serve)
    "$0" stop >/dev/null 2>&1 || true
    serve
    echo "server up again (pid $(cat "$dev/server.pid")) on :$port"
    ;;
  stop)
    [ -f "$dev/server.pid" ] && kill "$(cat "$dev/server.pid")" 2>/dev/null || true
    for pid in $(ss -ltnpH "sport = :$port" 2>/dev/null | grep -oE 'pid=[0-9]+' | cut -d= -f2 | sort -u); do kill "$pid" 2>/dev/null || true; done
    sleep 1
    echo stopped
    ;;
  status) ss -ltnH "sport = :$port" | grep -q . && echo "running on :$port" || echo "not running" ;;
  log) tail -n "${2:-40}" "$dev/server.log" ;;
  *) echo "usage: $0 start [trips] | serve | stop | status | log"; exit 2 ;;
esac
