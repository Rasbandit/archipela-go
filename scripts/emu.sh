#!/usr/bin/env bash
# Headless Android emulator for repeatable testing without a phone.
#   scripts/emu.sh create   one-time: install the system image and create the "apgo" AVD
#   scripts/emu.sh start    boot it (about 20 s with KVM) and wait until ready
#   scripts/emu.sh stop | status
# GPU mode swangle_indirect is required here: the other modes segfault the emulator on this machine.
set -euo pipefail
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_AVD_HOME="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
export JAVA_HOME="${JAVA_HOME:-/usr/lib/jvm/java-25-openjdk}"
sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
log="${TMPDIR:-/tmp}/apgo-emulator.log"

case "${1:-}" in
  create)
    "$sdk/sdkmanager" --sdk_root="$ANDROID_HOME" "emulator" "system-images;android-36;google_apis;x86_64"
    echo no | "$sdk/avdmanager" create avd -n apgo -k "system-images;android-36;google_apis;x86_64" -d pixel_8 --force
    ;;
  start)
    if adb devices | grep -q "^emulator-"; then echo "already running"; exit 0; fi
    setsid nohup "$ANDROID_HOME/emulator/emulator" -avd apgo -no-window -no-audio -no-snapshot -no-boot-anim \
      -gpu swangle_indirect -memory 3072 -no-metrics >"$log" 2>&1 </dev/null &
    for _ in $(seq 1 60); do
      sleep 5
      if [ "$(adb -s emulator-5554 shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then echo "emulator ready"; exit 0; fi
    done
    echo "emulator did not boot; see $log" >&2; exit 1
    ;;
  stop) adb -s emulator-5554 emu kill >/dev/null 2>&1 || true; echo stopped ;;
  status) adb devices | grep "^emulator-" || echo "not running" ;;
  *) echo "usage: $0 create|start|stop|status"; exit 2 ;;
esac
