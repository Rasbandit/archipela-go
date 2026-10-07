#!/usr/bin/env bash
# Tap a button on the connected phone by its visible label: scripts/android_tap.sh "Locate me"
set -euo pipefail
label="$1"
adb shell uiautomator dump /sdcard/apgo-ui.xml >/dev/null
adb pull -q /sdcard/apgo-ui.xml /tmp/apgo-ui.xml
python3 -I - "$label" <<'PY'
import re, subprocess, sys
import xml.etree.ElementTree as ET
label = sys.argv[1]
for n in ET.parse("/tmp/apgo-ui.xml").iter("node"):
    if n.get("text") == label or n.get("content-desc") == label:
        x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
        subprocess.run(["adb", "shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2)], check=True)
        print(f"tapped {label!r} at {(x1 + x2) // 2},{(y1 + y2) // 2}")
        break
else:
    sys.exit(f"no element labeled {label!r}")
PY
