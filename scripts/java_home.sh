#!/usr/bin/env bash
# Prints the JDK the Android build should use: $JAVA_HOME if set (an activated mise and CI set it), else on macOS what
# /usr/libexec/java_home reports, else the JDK that owns the javac on PATH (Gradle needs javac, so a bare JRE does not
# count). A javac that does not resolve to <jdk>/bin/javac next to a java (a mise shim, a readlink without -f) is ignored.
# Prints nothing when no JDK is found: gradlew then falls back to `java` on PATH. Never fails, because the justfile
# evaluates it for every recipe.
set -uo pipefail
if [ -n "${JAVA_HOME:-}" ]; then
  echo "$JAVA_HOME"
elif [ -x /usr/libexec/java_home ] && home="$(/usr/libexec/java_home 2>/dev/null)"; then
  echo "$home"
elif javac="$(command -v javac)" && real="$(readlink -f "$javac" 2>/dev/null)" && [ "${real%/bin/javac}" != "$real" ]; then
  home="${real%/bin/javac}"
  if [ -x "$home/bin/java" ]; then echo "$home"; fi
fi
exit 0
