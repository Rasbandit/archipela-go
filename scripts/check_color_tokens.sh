#!/usr/bin/env bash
# Every colour is a named token in ui/Palette.kt, so one file changes the app's look. Fails on colour literals anywhere else.
# Allowed elsewhere: Color.Transparent / Color.Unspecified (no colour) and .copy(alpha = ...) on a token.
set -euo pipefail

root="${1:-android/app/src/main/java}"
palette="dev/apgo2/ui/Palette.kt"
pattern='Color\(0x|Color\.(White|Black|Red|Green|Blue|Gray|Grey|DarkGray|LightGray|Yellow|Cyan|Magenta)\b'

hits=$(grep -rnE "$pattern" "$root" --include='*.kt' | grep -v "$palette" || true)
if [[ -n "$hits" ]]; then
    echo "Colour literals outside $palette (add or reuse an ApgoPalette token):" >&2
    echo "$hits" >&2
    exit 1
fi
