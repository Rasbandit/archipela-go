#!/usr/bin/env bash
# Git hooks export GIT_DIR (and friends) to the commands they run. A script test that builds a scratch repo must not
# follow those variables into the real one: once one committed its fixtures onto a real branch and set core.bare=true.
# This runs every other script test with the GIT_* variables aimed at a victim repo and checks the victim is untouched.
set -euo pipefail
# This test runs under hooks too: drop the inherited vars before building the victim, or it lands in the real repo.
# shellcheck disable=SC2046
unset $(git rev-parse --local-env-vars)
here="$(cd "$(dirname "$0")" && pwd)"
fail=0
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
v="$tmp/victim"
git init -q -b main "$v"
git -C "$v" -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit -q --allow-empty -m base
state() { git -C "$v" for-each-ref --format='%(refname) %(objectname)'; git -C "$v" config --get core.bare; git -C "$v" ls-files --stage; }
before="$(state)"
for t in "$here"/*_test.sh; do
  [ "$t" = "$here/git_env_test.sh" ] && continue
  name="$(basename "$t")"
  # pre-commit also exports GIT_INDEX_FILE, and worktree hooks can carry GIT_WORK_TREE: aim all three at the victim.
  verdict="ok"
  (cd "$v" && GIT_DIR="$v/.git" GIT_INDEX_FILE="$v/.git/index" GIT_WORK_TREE="$v" bash "$t" >/dev/null 2>&1) ||
    verdict="FAIL $name: failed under hook env"
  if [ "$(state)" != "$before" ]; then verdict="FAIL $name: changed the repo the hook vars point at"; before="$(state)"; fi
  if [ "$verdict" = ok ]; then echo "ok $name"; else echo "$verdict"; fail=1; fi
done
exit "$fail"
