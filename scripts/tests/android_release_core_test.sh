#!/usr/bin/env bash
# The release APK must carry a release-profile core: a debug core honours the dev simulator's flag (adversarial re-review N4).
# A Gradle dry run of assembleRelease must build the core with `android_core.sh release` before the release variant.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
plan="$(cd "$here/../../android" && ./gradlew -m :app:assembleRelease --console=plain -q)"
core="$(grep -n '^:app:releaseCore ' <<<"$plan" | cut -d: -f1 || true)"
pre="$(grep -n '^:app:preReleaseBuild ' <<<"$plan" | cut -d: -f1 || true)"
if [ -z "$core" ] || [ -z "$pre" ] || [ "$core" -gt "$pre" ]; then
  echo "FAIL release-core: assembleRelease does not build the release core first"
  echo "$plan" | head -20
  exit 1
fi
echo "ok release-core"
