#!/usr/bin/env python3
"""Summarise a pulled outing (scripts/pull_diag.sh): timeline, tracking gaps, errors, track length, raw track, GNSS.

usage: diag_report.py <pulled-dir>
"""

import contextlib
import json
import sqlite3
import sys
from collections.abc import Sequence
from datetime import datetime
from itertools import pairwise
from math import asin, cos, radians, sin, sqrt
from pathlib import Path
from typing import Any

LOCAL_TZ = datetime.now().astimezone().tzinfo


def is_number(v: object) -> bool:
    """A JSON number (not a bool, which Python counts as an int)."""
    return isinstance(v, int | float) and not isinstance(v, bool)


def ts(ms: int) -> str:
    return datetime.fromtimestamp(ms / 1000, tz=LOCAL_TZ).strftime("%H:%M:%S")


def haversine_m(a: Sequence[float], b: Sequence[float]) -> float:
    la1, lo1, la2, lo2 = map(radians, (a[0], a[1], b[0], b[1]))
    h = sin((la2 - la1) / 2) ** 2 + cos(la1) * cos(la2) * sin((lo2 - lo1) / 2) ** 2
    return 12_742_000 * asin(sqrt(h))


def report_raw_track(root: Path) -> None:
    """Debug builds only: the raw fixes, steps and compass lines the replay bench reads."""
    raw: list[dict[str, Any]] = []
    for f in sorted((root / "diag" / "raw").glob("raw-*.jsonl")):
        for line in f.read_text(errors="replace").splitlines():
            with contextlib.suppress(json.JSONDecodeError):
                e = json.loads(line)
                if isinstance(e, dict):
                    raw.append(e)
    fixes = [e for e in raw if e.get("tag") == "rawfix"]
    print(f"\n== raw track: {len(raw)} lines, {len(fixes)} fixes ==")
    if fixes:
        accs = sorted(e["acc"] for e in fixes if is_number(e.get("acc")))
        provs: dict[str, int] = {}
        for e in fixes:
            provs[e.get("prov", "?")] = provs.get(e.get("prov", "?"), 0) + 1
        print(
            f"providers {provs}, mock {sum(1 for e in fixes if e.get('mock'))},"
            f" with speed {sum(1 for e in fixes if e.get('spd') is not None)}"
        )
        if accs:
            print(
                f"accuracy m: median {accs[len(accs) // 2]:.1f}, p90 {accs[int(len(accs) * 0.9)]:.1f}"
            )
        print(
            f"steps lines {sum(1 for e in raw if e.get('tag') == 'rawsteps')},"
            f" heading lines {sum(1 for e in raw if e.get('tag') == 'rawhead')}"
        )


def report_gnss(entries: list[dict[str, Any]]) -> None:
    """The GNSS chip and the satellites it used (status lines are throttled by the app)."""
    gnss = [e for e in entries if e.get("tag") == "gnss"]
    status = [e for e in gnss if e.get("msg") == "status"]
    used = [e["used"] for e in status if is_number(e.get("used"))]
    print(f"\n== gnss: {len(status)} status lines ==")
    for e in gnss:
        if e.get("msg") == "hardware":
            print(f"hardware: {e.get('model')} {e.get('capabilities')}")
    if used:
        print(f"used satellites: mean {sum(used) / len(used):.1f} over {len(used)} lines")
    if status:
        print(f"dual frequency in {sum(1 for e in status if e.get('dual_freq'))} of {len(status)}")


def main(root: Path) -> None:  # noqa: C901, PLR0912  # linear one-shot report, splitting hurts readability
    entries: list[dict[str, Any]] = []
    for f in sorted((root / "diag").glob("diag-*.jsonl")):
        for line in f.read_text(errors="replace").splitlines():
            with contextlib.suppress(json.JSONDecodeError):
                entries.append(json.loads(line))
    print(
        f"{len(entries)} log entries"
        + (f", {ts(entries[0]['t'])} to {ts(entries[-1]['t'])}" if entries else "")
    )

    print("\n== errors, warnings and crashes ==")
    for e in entries:
        if e["lvl"] in ("E", "W"):
            print(
                f"{ts(e['t'])} {e['lvl']} {e['tag']}: {e['msg']}"
                + (f"\n    {e['stack'].splitlines()[0]}" if "stack" in e else "")
            )

    print("\n== timeline (lifecycle, sensors, game, quests) ==")
    for e in entries:
        if e["tag"] in (
            "lifecycle",
            "sensors",
            "service",
            "game",
            "event",
            "permission",
            "ap",
            "app",
        ):
            extra = {k: v for k, v in e.items() if k not in ("t", "lvl", "tag", "msg")}
            print(f"{ts(e['t'])} {e['tag']}: {e['msg']} {extra or ''}")

    beats = [e for e in entries if e["tag"] == "heartbeat"]
    print(f"\n== heartbeat: {len(beats)} beats ==")
    for a, b in pairwise(beats):
        if b["t"] - a["t"] > 90_000:
            print(
                f"GAP {ts(a['t'])} -> {ts(b['t'])} ({(b['t'] - a['t']) // 1000} s with no fixes: GPS off (home/car/stopped) or process frozen; see presence lines)"
            )
    for b in beats:
        if b["fixes"] == 0:
            print(
                f"{ts(b['t'])} no accepted fixes this minute (screen_on={b['screen_on']} doze={b['doze']} power_save={b['power_save']} last_acc={b['last_acc_m']})"
            )
    if beats:
        print(f"battery {beats[0]['battery_pct']}% -> {beats[-1]['battery_pct']}%")

    db = root / "files" / "journal.db"
    if db.exists():
        c = sqlite3.connect(db)
        pts = c.execute(
            "select t_ms, lat, lon, accuracy_m, simulated from points order by t_ms"
        ).fetchall()
        real = [p for p in pts if not p[4]]
        dist = sum(haversine_m(a[1:3], b[1:3]) for a, b in pairwise(real) if b[0] - a[0] <= 120_000)
        print(f"\n== journal: {len(pts)} points ({len(real)} real), walked {dist / 1000:.2f} km ==")
        if real:
            gaps = [(a[0], b[0]) for a, b in pairwise(real) if b[0] - a[0] > 120_000]
            print(
                f"{len(gaps)} gaps over 2 min in the real track"
                + "".join(f"\n  {ts(a)} -> {ts(b)} ({(b - a) // 60000} min)" for a, b in gaps[:20])
            )
            print(
                f"accuracy m: median {sorted(p[3] for p in real)[len(real) // 2]:.0f}, worst {max(p[3] for p in real):.0f}"
            )
        print(
            "events:",
            dict(c.execute("select kind, count(*) from events group by kind").fetchall()),
        )

    report_raw_track(root)
    report_gnss(entries)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(Path(sys.argv[1]))
