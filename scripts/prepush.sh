#!/usr/bin/env bash
# Pre-push: run only the `just check-*` recipes for what the pushed commits touch.
# `select` mode: paths on stdin -> recipe names on stdout ("__ALL__" = everything).
set -euo pipefail

select_recipes() {
  local py=0 rs=0 an=0 all=0 path
  while IFS= read -r path || [ -n "$path" ]; do
    case "$path" in
      "") ;;
      __ALL__|justfile|lefthook.yml|mise.toml|scripts/*|.github/*) all=1 ;;
      apworld/*) py=1 ;;
      core/*) rs=1; an=1 ;;
      android/*) an=1 ;;
    esac
  done
  if [ "$all" = 1 ]; then py=1; rs=1; an=1; fi
  local out="check-hygiene"
  [ "$py" = 1 ] && out+=" check-py"
  [ "$rs" = 1 ] && out+=" check-rust"
  [ "$an" = 1 ] && out+=" check-android"
  printf '%s\n' "$out"
}

diff_names() {
  git -c core.quotePath=false diff --name-only --no-renames "$1" "$2"
}

is_zero() { [[ "$1" =~ ^0+$ ]]; }
is_commit() { git cat-file -e "$1^{commit}" 2>/dev/null; }

# Fallback when git gave no refs (e.g. `lefthook run pre-push`): upstream, else merge-base with origin/main.
fallback_files() {
  local base files
  if base="$(git rev-parse --verify -q '@{upstream}')"; then :
  elif base="$(git merge-base HEAD origin/main 2>/dev/null)"; then :
  else echo "__ALL__"; return; fi
  files="$(diff_names "$base" HEAD)"
  # Nothing new relative to the base still pushes; check everything to be safe.
  if [ -z "$files" ] && [ "$(git rev-parse HEAD)" = "$base" ]; then echo "__ALL__"; return; fi
  printf '%s\n' "$files"
}

# stdin: git pre-push lines "<local ref> <local sha> <remote ref> <remote sha>" -> union of changed paths.
pushed_files() {
  local input lsha rsha base
  input="$(cat)"
  if ! printf '%s' "$input" | grep -q '[^[:space:]]'; then fallback_files; return; fi
  while read -r _ lsha _ rsha; do
    [ -n "$lsha" ] || continue
    if is_zero "$lsha"; then continue; fi # remote delete: nothing pushed
    if ! is_commit "$lsha"; then echo "__ALL__"; continue; fi # e.g. tag of a non-commit
    if is_zero "$rsha"; then
      base="$(git merge-base "$lsha" origin/main 2>/dev/null)" || { echo "__ALL__"; continue; }
    elif is_commit "$rsha"; then base="$rsha"
    else echo "__ALL__"; continue; fi
    diff_names "$base" "$lsha"
  done <<<"$input"
  return 0
}

case "${1:-}" in
  select) select_recipes; exit ;;
  files) pushed_files; exit ;;
esac
# A terminal on stdin means a manual run: no refs to read.
if [ -t 0 ]; then recipes="$(fallback_files | select_recipes)"; else recipes="$(pushed_files | select_recipes)"; fi
echo "pre-push: just $recipes"
# shellcheck disable=SC2086  # word-splitting is the recipe list
exec just $recipes
