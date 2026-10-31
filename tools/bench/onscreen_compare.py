#!/usr/bin/env python3
"""Runs `onscreen.py` of each workload in each terminal, in turns, each run in
a window of its own, of the configs of a directory, and writes one JSON line
a run, with its terminal:

    python3 tools/bench/onscreen_compare.py <workloads> <configs> <out.jsonl> [<runs>]

The configs' directory holds `lokyr/config.toml`, `ghostty.conf`,
`alacritty.toml` and `kitty.conf`; iTerm2 takes the dynamic profile
`Lokyr Bench`. Lokyr runs through `--bench output`, whose frames' report
goes with its runs. A terminal that is not installed is left out.
"""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
ONSCREEN = os.path.join(HERE, "onscreen.py")
PYTHON = "/usr/bin/python3"
WORKLOADS = ["ascii.txt", "seq.txt", "sgr.txt", "cjk.txt"]
TIMEOUT = 300


def lokyr(configs, script):
    binary = os.path.join(ROOT, "target/Lokyr.app/Contents/MacOS/lokyr")
    env = dict(os.environ, XDG_CONFIG_HOME=configs)
    return [binary, "--bench", "output", "--window", "1920x1080", "--", *script], env


def ghostty(configs, script):
    binary = "/Applications/Ghostty.app/Contents/MacOS/ghostty"
    config = os.path.join(configs, "ghostty.conf")
    return [binary, "--config-default-files=false", f"--config-file={config}", "-e", *script], None


def alacritty(configs, script):
    binary = "/tmp/lokyr/alacritty/src/target/release/osx/Alacritty.app/Contents/MacOS/alacritty"
    config = os.path.join(configs, "alacritty.toml")
    return [binary, "--config-file", config, "-e", *script], None


def kitty(configs, script):
    binary = "/Applications/kitty.app/Contents/MacOS/kitty"
    config = os.path.join(configs, "kitty.conf")
    return [binary, "--config", config, *script], None


def iterm2(configs, script):
    command = " ".join(script)
    apple_script = (
        'tell application "iTerm" to create window with profile "Lokyr Bench" '
        f'command "{command}"'
    )
    return ["/usr/bin/osascript", "-e", apple_script], None


TERMINALS = [
    ("lokyr", lokyr, os.path.join(ROOT, "target/Lokyr.app")),
    ("ghostty", ghostty, "/Applications/Ghostty.app"),
    ("alacritty", alacritty, "/tmp/lokyr/alacritty/src/target/release/osx/Alacritty.app"),
    ("kitty", kitty, "/Applications/kitty.app"),
    ("iterm2", iterm2, "/Applications/iTerm.app"),
]


def lines(path):
    if not os.path.exists(path):
        return 0
    with open(path) as file:
        return sum(1 for _ in file)


def main():
    workloads, configs, out = sys.argv[1], sys.argv[2], sys.argv[3]
    runs = int(sys.argv[4]) if len(sys.argv) > 4 else 3
    # `ONSCREEN_TERMINALS`, as `lokyr,ghostty`, keeps those alone.
    chosen = os.environ.get("ONSCREEN_TERMINALS")
    terminals = [
        t
        for t in TERMINALS
        if os.path.exists(t[2]) and (chosen is None or t[0] in chosen.split(","))
    ]
    scratch = out + ".run"
    for run in range(runs):
        for workload in WORKLOADS:
            # The terminals' order turns, against the heat.
            turn = run * len(WORKLOADS) + WORKLOADS.index(workload)
            order = terminals[turn % len(terminals):] + terminals[: turn % len(terminals)]
            for name, command, _ in order:
                if os.path.exists(scratch):
                    os.remove(scratch)
                path = os.path.join(workloads, workload)
                # A script of no arguments, which every terminal runs alike.
                wrapper = out + ".sh"
                with open(wrapper, "w") as file:
                    file.write(f"#!/bin/sh\nexec {PYTHON} '{ONSCREEN}' '{path}' '{scratch}'\n")
                os.chmod(wrapper, 0o755)
                argv, env = command(configs, [wrapper])
                start = time.time()
                process = subprocess.Popen(
                    argv, env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
                )
                while lines(scratch) < 1 and time.time() - start < TIMEOUT:
                    time.sleep(0.2)
                frames = None
                if name == "lokyr":
                    stdout, _ = process.communicate(timeout=60)
                    text = stdout.decode().strip()
                    frames = json.loads(text) if text.startswith("{") else None
                else:
                    try:
                        process.wait(timeout=30)
                    except subprocess.TimeoutExpired:
                        process.terminate()
                if lines(scratch) < 1:
                    print(f"{name} {workload}: no result", file=sys.stderr)
                    continue
                with open(scratch) as file:
                    result = json.loads(file.readline())
                result["terminal"] = name
                result["run"] = run + 1
                if frames:
                    result["frames"] = frames
                with open(out, "a") as file:
                    file.write(json.dumps(result) + "\n")
                print(f"{name} {workload} run {run + 1}: {result['mib_s']} MiB/s", file=sys.stderr)
                time.sleep(2)


if __name__ == "__main__":
    main()
