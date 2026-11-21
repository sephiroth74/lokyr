#!/usr/bin/env python3
"""The figures of a window's motion, of an Animation Hitches recording of
every process that ran across it, `xctrace record --all-processes`, and of
the report of `lokyr-bench motion`, which places it in the recording:

    python3 tools/bench/motion_extract.py <trace> <motion.json> <pid> [refresh_ms]

Prints JSON, over the motion less its first and its last 0.3 s, within
the recording: of the
display that showed the most frames, its frames a second, the period of
its refresh, the frames that it missed, as the gaps between two longer
than a refresh and a half count them, and the compositor's delay of each
frame, its median and its 95th percentile, in ms; and the CPU of the
window server and of the process `pid`, in percent of a core.

With `refresh_ms`, the display's refresh, as a scroll's has it, whose
display shows frames only as they change: the frames that the process
presented, a second, and the share of the display's refreshes within
which it presented one, of the refreshes' grid from the motion's start,
and the gaps between its frames, in refreshes.
"""
import json
import os
import statistics
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import xctrace_tables  # noqa: E402

TRIM = 0.3


def main():
    trace, report, pid = sys.argv[1], sys.argv[2], sys.argv[3]
    refresh = float(sys.argv[4]) * 1e6 if len(sys.argv) > 4 else None
    motion = json.load(open(report))
    origin = xctrace_tables.start(trace)
    # A motion that began before the recording did is measured from the
    # recording's start.
    first = max(motion["began"] - origin, 0.0) * 1e9 + TRIM * 1e9
    last = (motion["ended"] - origin - TRIM) * 1e9
    seconds = (last - first) / 1e9
    swaps = {}
    for row in xctrace_tables.table(trace, "display-surface-swap"):
        at = int(row["timestamp"][0])
        if first <= at <= last:
            display = row["display-name"][1]
            swaps.setdefault(display, []).append((at, int(row["delay"][0])))
    display, shown = max(swaps.items(), key=lambda item: len(item[1]))
    times = [at for at, _ in shown]
    gaps = [b - a for a, b in zip(times, times[1:])]
    period = statistics.median(gaps)
    missed = sum(max(0, round(gap / period) - 1) for gap in gaps)
    delays = sorted(delay / 1e6 for _, delay in shown)
    cpu = {"WindowServer": 0.0, "app": 0.0}
    for row in xctrace_tables.table(trace, "ProcessCPUUsage"):
        at = int(row["start"][0])
        if not first <= at <= last:
            continue
        process = row["process"][1] or ""
        usage = float(row["cpu-usage"][0]) * int(row["duration"][0]) / 1e9
        if process.startswith("WindowServer ("):
            cpu["WindowServer"] += usage
        elif process.endswith(f"({pid})"):
            cpu["app"] += usage
    presented = {}
    if refresh:
        presents = sorted(
            int(row["timestamp"][0])
            for row in xctrace_tables.table(trace, "metal-command-buffer-frame-assignment")
            if row["pid"][0] == pid and row["has-present"][0] == "1"
            and first <= int(row["timestamp"][0]) <= last
        )
        refreshes = int((last - first) // refresh)
        within = {int((at - first) // refresh) for at in presents}
        gaps = {}
        for a, b in zip(presents, presents[1:]):
            count = min(round((b - a) / refresh), 6)
            gaps[str(count)] = gaps.get(str(count), 0) + 1
        presented = {
            "app_frames_per_second": round(len(presents) / seconds, 1),
            "refreshes_with_a_frame": round(len(within & set(range(refreshes))) / max(refreshes, 1), 3),
            "app_frame_gaps": dict(sorted(gaps.items())),
        }
    print(json.dumps({
        "kind": motion["kind"],
        "farthest": motion.get("farthest"),
        "seconds": round(seconds, 2),
        "display": display,
        "frames_per_second": round(len(shown) / seconds, 1),
        "refresh_ms": round(period / 1e6, 2),
        "missed": missed,
        "delay_p50_ms": round(statistics.median(delays), 2),
        "delay_p95_ms": round(delays[int(0.95 * (len(delays) - 1))], 2),
        "windowserver_cpu": round(cpu["WindowServer"] / seconds, 1),
        "app_cpu": round(cpu["app"] / seconds, 1),
        **presented,
    }))


if __name__ == "__main__":
    main()
