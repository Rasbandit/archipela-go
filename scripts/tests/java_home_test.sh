#!/usr/bin/env bash
# Tests for scripts/java_home.sh: which JDK the dev loop hands to Gradle.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sut="$here/../java_home.sh"
fail=0
bash="$(command -v bash)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
expect() {
  local name="$1" want="$2" got
  shift 2
  got="$(env -i "$@" "$bash" "$sut")" || { echo "FAIL $name: non-zero exit"; fail=1; return; }
  if [ "$got" != "$want" ]; then echo "FAIL $name: want '$want' got '$got'"; fail=1; else echo "ok $name"; fi
}

# A fake JDK whose javac is reached through a symlink, like Fedora's alternatives.
mkdir -p "$tmp/jdk/bin" "$tmp/path" "$tmp/tools"
# Only the tools the script needs, so a real javac on the host never leaks in.
for t in readlink dirname; do ln -s "$(command -v "$t")" "$tmp/tools/$t"; done
printf '#!/bin/sh\n' >"$tmp/jdk/bin/javac"
cp "$tmp/jdk/bin/javac" "$tmp/jdk/bin/java"
chmod +x "$tmp/jdk/bin/javac" "$tmp/jdk/bin/java"
ln -s "$tmp/jdk/bin/javac" "$tmp/path/javac"

expect env-wins      "/opt/my-jdk" JAVA_HOME=/opt/my-jdk PATH="$tmp/path:$tmp/tools"
expect from-javac    "$tmp/jdk"    PATH="$tmp/path:$tmp/tools"
expect empty-env     "$tmp/jdk"    JAVA_HOME= PATH="$tmp/path:$tmp/tools"
expect no-jdk        ""            PATH="$tmp/tools"

# A mise shim: javac on PATH is a symlink to the mise binary, not to a JDK. Resolving it must not invent a JAVA_HOME.
mkdir -p "$tmp/mise/bin" "$tmp/shims"
printf '#!/bin/sh\n' >"$tmp/mise/bin/mise"
chmod +x "$tmp/mise/bin/mise"
ln -s "$tmp/mise/bin/mise" "$tmp/shims/javac"
expect mise-shim     ""            PATH="$tmp/shims:$tmp/tools"

# A readlink without -f (old macOS) fails: print nothing, not ".".
mkdir -p "$tmp/badtools"
ln -s "$(command -v dirname)" "$tmp/badtools/dirname"
printf '#!/bin/sh\nexit 1\n' >"$tmp/badtools/readlink"
chmod +x "$tmp/badtools/readlink"
expect no-readlink-f ""            PATH="$tmp/path:$tmp/badtools"

exit "$fail"
