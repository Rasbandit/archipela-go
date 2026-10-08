#!/usr/bin/env bash
# Tests for scripts/prepush.sh select: changed paths -> just recipes.
set -euo pipefail
# A git hook exports GIT_DIR and friends; drop them so the scratch repo below never resolves to the real one.
# shellcheck disable=SC2046
unset $(git rev-parse --local-env-vars)
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
# core embeds these with include_str!, so they feed the Rust (and Android) builds too
expect apworld-docs "apworld/docs/slot_data.sample.json"      "$ALL"
expect ap-version  ".ap-version"                              "check-hygiene check-py"

# --- range logic: `prepush.sh files` (pre-push stdin lines -> changed paths) ---
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
git_() { git -C "$tmp/repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false "$@"; }
commit_file() { mkdir -p "$(dirname "$tmp/repo/$1")"; echo "$2" >"$tmp/repo/$1"; git_ add -A; git_ commit -qm "$1"; git_ rev-parse HEAD; }
git init -q -b main "$tmp/repo"
c0="$(commit_file README.md base)"
git_ update-ref refs/remotes/origin/main "$c0"
c1="$(commit_file apworld/a.py one)"
c2="$(commit_file android/b.kt two)"
c3="$(commit_file "docs/é.md" three)"
Z=0000000000000000000000000000000000000000
files() { (cd "$tmp/repo" && printf '%s' "$1" | bash "$sut" files | sort | tr '\n' ' '); }
expect_files() {
  local name="$1" input="$2" want="$3" got
  got="$(files "$input")"
  if [ "$got" != "$want" ]; then echo "FAIL $name: want '$want' got '$got'"; fail=1; else echo "ok $name"; fi
}
expect_files push-range    "refs/heads/x $c2 refs/heads/x $c1"                  "android/b.kt "
expect_files new-branch    "refs/heads/x $c2 refs/heads/x $Z"                   "android/b.kt apworld/a.py "
expect_files delete        "(delete) $Z refs/heads/x $c1"                       ""
expect_files unknown-remote "refs/heads/x $c2 refs/heads/x $(printf 'f%.0s' {1..40})" "__ALL__ "
expect_files multi-refs    "refs/heads/x $c2 refs/heads/x $c1"$'\n'"refs/heads/y $c3 refs/heads/y $c2" "android/b.kt docs/é.md "
expect_files non-ascii     "refs/heads/x $c3 refs/heads/x $c2"                  "docs/é.md "
expect_files tag-nontcommit "refs/tags/t $(git_ rev-parse "$c0^{tree}") refs/tags/t $Z" "__ALL__ "
# empty stdin: fall back to upstream/merge-base; with neither -> all
git_ update-ref -d refs/remotes/origin/main
expect_files empty-no-base ""                                                   "__ALL__ "
git_ update-ref refs/remotes/origin/main "$c0"
expect_files empty-merge-base ""                                                "android/b.kt apworld/a.py docs/é.md "
exit $fail
