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

changed_files() {
  local base
  if base="$(git rev-parse --verify -q '@{upstream}')"; then :
  elif base="$(git merge-base HEAD origin/main 2>/dev/null)"; then :
  else echo "__ALL__"; return; fi
  local files
  files="$(git diff --name-only "$base" HEAD)"
  # New branch whose upstream equals HEAD (nothing new) still pushes; check everything to be safe.
  if [ -z "$files" ] && [ "$(git rev-parse HEAD)" = "$base" ]; then echo "__ALL__"; return; fi
  printf '%s\n' "$files"
}

if [ "${1:-}" = select ]; then select_recipes; exit; fi
recipes="$(changed_files | select_recipes)"
echo "pre-push: just $recipes"
# shellcheck disable=SC2086  # word-splitting is the recipe list
exec just $recipes
