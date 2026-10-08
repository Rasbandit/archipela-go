#!/usr/bin/env bash
# Prints the JDK the Android build should use: $JAVA_HOME if set (mise and CI set it), else the JDK that owns the
# javac on PATH (Gradle needs javac, so a bare JRE does not count). Prints nothing when neither exists: gradlew then
# falls back to `java` on PATH. Never fails, because the justfile evaluates it for every recipe.
set -uo pipefail
if [ -n "${JAVA_HOME:-}" ]; then
  echo "$JAVA_HOME"
elif javac="$(command -v javac)"; then
  dirname "$(dirname "$(readlink -f "$javac")")"
fi
exit 0
