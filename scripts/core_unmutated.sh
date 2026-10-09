#!/usr/bin/env bash
# Fails while a mutation from an interrupted `just mutate-rust` is left in the core (cargo-mutants marks the line).
# Usage: scripts/core_unmutated.sh [repo-root]
set -euo pipefail
root="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
if grep -rn "changed by cargo-mutants" "$root/core/src" "$root/core/ffi/src"; then
  echo "a mutation from an interrupted 'just mutate-rust' is left in: git restore core/src" >&2
  exit 1
fi
