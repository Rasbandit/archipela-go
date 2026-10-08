#!/usr/bin/env python3
"""Drive the app over adb by visible text (honors ANDROID_SERIAL).

android_ui.py texts                 print visible texts
android_ui.py tap "Play solo"       tap a label (scrolls down to find it)
android_ui.py wait "Scan done" 300  wait for a text to appear (seconds)
"""

import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET


def adb(*args: str, timeout: int = 30) -> str:
    return subprocess.run(
        ["adb", *args], capture_output=True, text=True, timeout=timeout, check=False
    ).stdout


def nodes() -> list[dict]:
    adb("shell", "uiautomator", "dump", "/sdcard/ui.xml")
    adb("pull", "/sdcard/ui.xml", "/tmp/apgo-ui2.xml")
    out = []
    for n in ET.parse("/tmp/apgo-ui2.xml").iter("node"):  # noqa: S314  # our own adb uiautomator dump
        text = n.get("text") or n.get("content-desc") or ""
        b = re.findall(r"\d+", n.get("bounds") or "")
        if text and len(b) == 4:
            out.append({"text": text, "box": tuple(map(int, b))})
    return out


def screen_h() -> int:
    m = re.search(r"(\d+)x(\d+)", adb("shell", "wm", "size"))
    return int(m.group(2)) if m else 2400


NAV = {"Realms", "New Game", "Play"}


def tap(label: str, *, exact: bool = False, nth: int = 0) -> bool:
    h = screen_h()
    limit = h - 20 if label in NAV else h - 260
    for _ in range(8):
        seen = 0
        for n in nodes():
            hit = n["text"] == label if exact else label in n["text"]
            x1, y1, x2, y2 = n["box"]
            if hit and y1 > 0 and y2 < limit and y2 > y1:
                if seen == nth:
                    adb(
                        "shell",
                        "input",
                        "tap",
                        str((x1 + x2) // 2),
                        str((y1 + y2) // 2),
                    )
                    return True
                seen += 1
        adb(
            "shell",
            "input",
            "swipe",
            "540",
            str(int(h * 0.75)),
            "540",
            str(int(h * 0.30)),
            "250",
        )
        time.sleep(0.5)
    return False


def main() -> int:
    cmd = sys.argv[1]
    if cmd == "texts":
        print(" | ".join(n["text"] for n in nodes()))
    elif cmd == "tap":
        ok = tap(sys.argv[2], exact=len(sys.argv) > 3)
        print("tapped" if ok else f"NOT FOUND: {sys.argv[2]}")
        return 0 if ok else 1
    elif cmd == "tapn":
        ok = tap(sys.argv[2], exact=True, nth=int(sys.argv[3]))
        print("tapped" if ok else f"NOT FOUND: {sys.argv[2]}[{sys.argv[3]}]")
        return 0 if ok else 1
    elif cmd == "type":
        # replace the text of the field currently showing `old` with `new`
        if not tap(sys.argv[2], exact=True):
            return 1
        time.sleep(0.5)
        adb("shell", "input", "keyevent", "KEYCODE_MOVE_END")
        for _ in range(len(sys.argv[2]) + 2):
            adb("shell", "input", "keyevent", "KEYCODE_DEL")
        adb("shell", "input", "text", sys.argv[3])
        adb("shell", "input", "keyevent", "KEYCODE_BACK")
        print("typed")
    elif cmd == "wait":
        end = time.time() + float(sys.argv[3])
        while time.time() < end:
            texts = " | ".join(n["text"] for n in nodes())
            if sys.argv[2] in texts:
                print(texts[:800])
                return 0
            time.sleep(3)
        print("TIMEOUT")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
