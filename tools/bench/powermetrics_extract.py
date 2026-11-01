#!/usr/bin/env python3
"""The figures of a `powermetrics --samplers tasks,cpu_power,gpu_power`
recording that the comparison's report reads, out of its text, of each
sample:

    python3 tools/bench/powermetrics_extract.py <powermetrics.txt> <sequence.log> <out>

`<sequence.log>` holds a line `<epoch> <terminal> <phase>` as each
terminal opens, rests, starts and ends its `cat`, and closes. `<out>` gets
`samples.csv`, the whole system's power of each sample, and `tasks.csv`, the
rows of WindowServer and of the processes of the terminals that the log
names, those that started while it ran.

Each sample's header tells its start to the second, and its length to the
hundredth of a millisecond: the starts are those of the first sample
plus the lengths before, which the headers' seconds bound within a
millisecond.
"""
import csv
import datetime
import os
import re
import sys

# The processes of each terminal, by the names that powermetrics gives them.
PROCESSES = {
    "lokyr": ["lokyr"],
    "ghostty": ["ghostty"],
    "alacritty": ["alacritty"],
    "kitty": ["kitty"],
    "wezterm": ["wezterm-gui"],
    "iterm2": ["iTerm2", "iTermServer"],
    "terminal": ["Terminal"],
}


def samples_of(path):
    samples = []
    section = None
    with open(path) as file:
        for line in file:
            header = re.match(r"\*\*\* Sampled system activity \((.+?)\) \(([\d.]+)ms elapsed\)",
                              line)
            if header:
                second = datetime.datetime.strptime(header.group(1),
                                                    "%a %b %d %H:%M:%S %Y %z").timestamp()
                samples.append({"second": second, "elapsed_ms": float(header.group(2)),
                                "tasks": []})
                section = None
            elif line.startswith("*** Running tasks"):
                section = "tasks"
            elif line.startswith("**** Processor usage"):
                section = "cpu"
            elif line.startswith("**** GPU usage"):
                section = "gpu"
            elif section == "tasks" and len(line) > 40 and not line.startswith("Name"):
                # The name takes the first 35 columns, and may hold spaces.
                fields = line[35:].split()
                if len(fields) == 9:
                    samples[-1]["tasks"].append((line[:35].strip(), int(fields[0]),
                                                 *map(float, fields[1:])))
            elif section == "cpu" and (power := re.match(r"(CPU|GPU) Power: (\d+) mW", line)):
                samples[-1][power.group(1).lower() + "_mw"] = int(power.group(2))
            elif section == "gpu" and (busy := re.match(r"GPU HW active residency:\s+([\d.]+)%",
                                                        line)):
                samples[-1]["gpu_active"] = float(busy.group(1))
    # The first sample's start, which every header's second bounds.
    low, high, before = -float("inf"), float("inf"), 0.0
    for sample in samples:
        low = max(low, sample["second"] - before)
        high = min(high, sample["second"] + 1 - before)
        sample["before"] = before
        before += sample["elapsed_ms"] / 1000
    first = (low + high) / 2
    for sample in samples:
        sample["start"] = first + sample["before"]
    return samples


def sequence_of(path):
    phases = {}
    with open(path) as file:
        for line in file:
            parts = line.split()
            if len(parts) == 3:
                phases[(parts[1], parts[2])] = float(parts[0])
    return phases


def main():
    recording, log, out = sys.argv[1:4]
    samples = samples_of(recording)
    phases = sequence_of(log)
    # Each terminal's own processes: of its names, first seen between its
    # opening and its closing.
    first_seen = {}
    for sample in samples:
        for task in sample["tasks"]:
            first_seen.setdefault((task[0], task[1]), sample["start"] + sample["elapsed_ms"] / 1000)
    owner = {}
    for (name, pid), seen in first_seen.items():
        for terminal, names in PROCESSES.items():
            if (terminal, "opens") not in phases:
                continue
            if (any(name.startswith(prefix) for prefix in names)
                    and phases[(terminal, "opens")] <= seen <= phases[(terminal, "closed")] + 1.5):
                owner[pid] = terminal
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "samples.csv"), "w", newline="") as file:
        writer = csv.writer(file)
        writer.writerow(["start", "elapsed_ms", "cpu_mw", "gpu_mw", "gpu_active_percent"])
        for sample in samples:
            writer.writerow([f"{sample['start']:.3f}", sample["elapsed_ms"], sample.get("cpu_mw"),
                             sample.get("gpu_mw"), sample.get("gpu_active")])
    with open(os.path.join(out, "tasks.csv"), "w", newline="") as file:
        writer = csv.writer(file)
        writer.writerow(["start", "terminal", "process", "pid", "cpu_ms_per_s", "user_percent",
                         "interrupt_wakeups_per_s", "package_idle_wakeups_per_s",
                         "energy_impact"])
        for sample in samples:
            for name, pid, cpu, user, _, _, interrupts, idle, _, energy in sample["tasks"]:
                terminal = "" if name == "WindowServer" else owner.get(pid)
                if terminal is None:
                    continue
                writer.writerow([f"{sample['start']:.3f}", terminal, name, pid, cpu, user,
                                 interrupts, idle, energy])


if __name__ == "__main__":
    main()
