#!/usr/bin/env bash
# Git hooks export GIT_DIR (and friends) to the commands they run. A script test that builds a scratch repo must not
# follow those variables into the real one: once one committed its fixtures onto a real branch and set core.bare=true.
# This runs every other script test with the hook variables aimed at a victim and checks the victim is untouched.
# Two victims: a plain repo, and a linked worktree of it. Only the worktree reproduces the core.bare=true flip, because
# git guesses "not bare" for any GIT_DIR ending in /.git.
set -euo pipefail
# This test runs under hooks too: drop the inherited vars before building the victims, or they land in the real repo.
# shellcheck disable=SC2046
unset $(git rev-parse --local-env-vars)
here="$(cd "$(dirname "$0")" && pwd)"
fail=0
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
v="$tmp/victim"
wt="$tmp/wt"
git init -q -b main "$v"
git -C "$v" -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit -q --allow-empty -m base
git -C "$v" worktree add -q -b wt "$wt"
state() {
  git -C "$v" for-each-ref --format='%(refname) %(objectname)'
  git -C "$v" config --get core.bare
  git -C "$v" ls-files --stage
  git -C "$wt" ls-files --stage
}
before="$(state)"

# run_as <label> <cwd> <test> VAR=value...: run a test from <cwd> with the given hook variables exported.
# Do not add GIT_WORK_TREE: hooks don't export it, and setting it stops git guessing "bare", hiding the core.bare leak.
run_as() {
  local verdict="ok" label="$1" cwd="$2" t="$3" name
  shift 3
  name="$(basename "$t")"
  (cd "$cwd" && env "$@" bash "$t" >/dev/null 2>&1) || verdict="FAIL $name ($label): failed under hook env"
  if [ "$(state)" != "$before" ]; then
    verdict="FAIL $name ($label): changed the repo the hook vars point at"
    git -C "$v" config core.bare false # undo the worst damage so later runs start from a sane victim
    before="$(state)"
  fi
  if [ "$verdict" = ok ]; then echo "ok $name ($label)"; else echo "$verdict"; fail=1; fi
}

g="$v/.git"
wg="$v/.git/worktrees/wt"
for t in "$here"/*_test.sh; do
  [ "$t" = "$here/git_env_test.sh" ] && continue
  # pre-push exports GIT_DIR; pre-commit adds GIT_INDEX_FILE.
  run_as "repo, pre-push" "$v" "$t" GIT_DIR="$g"
  run_as "repo, pre-commit" "$v" "$t" GIT_DIR="$g" GIT_INDEX_FILE="$g/index"
  run_as "worktree, pre-push" "$wt" "$t" GIT_DIR="$wg"
  run_as "worktree, pre-commit" "$wt" "$t" GIT_DIR="$wg" GIT_INDEX_FILE="$wg/index"
done
exit "$fail"
