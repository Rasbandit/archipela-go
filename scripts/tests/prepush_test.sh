#!/usr/bin/env bash
# Tests for scripts/prepush.sh select: changed paths -> just recipes.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sut="$here/../prepush.sh"
fail=0
expect() {
  local name="$1" input="$2" want="$3" got
  got="$(printf '%s' "$input" | bash "$sut" select)"
  if [ "$got" != "$want" ]; then echo "FAIL $name: want '$want' got '$got'"; fail=1; else echo "ok $name"; fi
}
ALL="check-hygiene check-py check-rust check-android"
expect no-range    "__ALL__"                                  "$ALL"
expect docs-only   "docs/a.md"                                "check-hygiene"
expect py          "apworld/ap_go2/x.py"                      "check-hygiene check-py"
expect deleted-py  "apworld/ap_go2/gone.py"                   "check-hygiene check-py"
expect rust-core   "core/src/lib.rs"                          "check-hygiene check-rust check-android"
expect ffi-only    "core/ffi/src/lib.rs"                      "check-hygiene check-rust check-android"
expect kotlin      "android/app/src/main/java/dev/apgo2/A.kt" "check-hygiene check-android"
expect justfile    "justfile"                                 "$ALL"
expect mixed       $'apworld/a.py\nandroid/b.kt'              "check-hygiene check-py check-android"
expect empty       ""                                         "check-hygiene"
exit $fail
