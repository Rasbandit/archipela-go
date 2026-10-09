#!/usr/bin/env bash
# Tests for scripts/pull_diag.sh against a fake phone: missing folders and file names with spaces.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sut="$here/../pull_diag.sh"
fail=0
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# A fake adb that, like the real one, joins its arguments into one string for the phone's shell (so unquoted names split there too).
# Like an adb without the shell protocol, the phone's stderr comes back on stdout. The phone is a folder; run-as runs its command in it.
mkdir -p "$tmp/bin"
cat >"$tmp/bin/adb" <<'EOF'
#!/usr/bin/env bash
sub="$1"
shift
case "$sub" in
  shell | exec-out) cd "$FAKE_PHONE" && PATH="$FAKE_BIN:$PATH" sh -c "$*" 2>&1 ;;
  *) exit 1 ;; # pull, logcat: no external storage, no device log
esac
EOF
printf '#!/bin/sh\nshift\nexec "$@"\n' >"$tmp/bin/run-as"
chmod +x "$tmp/bin/adb" "$tmp/bin/run-as"

run() { # phone-dir out-dir
  FAKE_PHONE="$1" FAKE_BIN="$tmp/bin" PATH="$tmp/bin:$PATH" bash "$sut" "$2" >"$2.log" 2>&1
}
check() { # name condition...
  local name="$1"
  shift
  if "$@"; then echo "ok $name"; else echo "FAIL $name"; fail=1; fi
}

# A phone with only a journal: no games, atlas or diag folders.
mkdir -p "$tmp/bare/files"
echo journal >"$tmp/bare/files/journal.db"
if run "$tmp/bare" "$tmp/out-bare"; then echo "ok missing-folders-exit-0"; else echo "FAIL missing-folders-exit-0"; cat "$tmp/out-bare.log"; fail=1; fi
check missing-folders-no-junk [ -z "$(find "$tmp/out-bare/files/games" "$tmp/out-bare/files/atlas" "$tmp/out-bare/diag" -type f 2>/dev/null)" ]
check missing-folders-journal [ "$(cat "$tmp/out-bare/files/journal.db" 2>/dev/null)" = journal ]

# File names with spaces in every pulled folder.
mkdir -p "$tmp/full/files/games" "$tmp/full/files/atlas" "$tmp/full/files/diag/raw"
echo game >"$tmp/full/files/games/g 1.json"
echo atlas >"$tmp/full/files/atlas/my realm.json"
echo log >"$tmp/full/files/diag/diag 0001.jsonl"
echo raw >"$tmp/full/files/diag/raw/raw 0001.jsonl"
if run "$tmp/full" "$tmp/out-full"; then echo "ok spaces-exit-0"; else echo "FAIL spaces-exit-0"; cat "$tmp/out-full.log"; fail=1; fi
check spaces-game [ "$(cat "$tmp/out-full/files/games/g 1.json" 2>/dev/null)" = game ]
check spaces-atlas [ "$(cat "$tmp/out-full/files/atlas/my realm.json" 2>/dev/null)" = atlas ]
check spaces-diag [ "$(cat "$tmp/out-full/diag/diag 0001.jsonl" 2>/dev/null)" = log ]
check spaces-raw [ "$(cat "$tmp/out-full/diag/raw/raw 0001.jsonl" 2>/dev/null)" = raw ]

exit "$fail"
