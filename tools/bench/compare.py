#!/usr/bin/env python3
"""Lokyr against the other terminals of macOS, each under the same
conditions: the configs of a directory, of Menlo at 13 points in 120 by 40
cells, and a plain zsh, whose ZDOTDIR is an empty directory, so that no
terminal reads the user's.

    python3 tools/bench/compare.py <measure> <configs> <out> [<terminals>]

`<configs>` holds `lokyr/`, `ghostty/`, `alacritty/`, `kitty/` and
`wezterm/`, as an XDG config directory does, and `iterm2-profile.json`;
`<out>` takes each measure's results; `<terminals>`, as `lokyr,ghostty`,
keeps those alone. The measures:

- `check`: each terminal's grid, as `stty size` tells it, and its shell's
  prompt, which must be 40 rows of 120 columns and the system's.
- `setup`: the configs into an XDG directory; iTerm2's dynamic profile and
  a profile of Terminal.app, as their defaults, the user's settings of
  both saved first, which `restore` puts back.
- `startup`: each terminal's first window, with `lokyr-bench startup`, a
  run to warm up and five, twice, the terminals' order turning; and
  Lokyr's first frame.
- `idle`: each terminal alone, its wakeups for 60 s and its energy over
  them, then its memory: summed over the processes of the app's bundle,
  as iTerm2's servers, without the shell.
- `throughput`: in each terminal, kitty's benchmark without and with
  rendering, three times each, but in iTerm2, which never answers the
  query that ends its tests; vtebench, and `cat` of 1 GiB ended with a
  query of the cursor, five times.
- `energy`: each terminal's energy over 10 s of `yes`, the CPU's alone,
  as the kernel bills it.
- `energycat`: each terminal's energy over `cat` of 1 GiB, which takes
  the same bytes in each, in joules a GiB, the CPU's alone.
- `hud`: the Metal HUD's log of each terminal over 10 s of scrolling.
- `ucs`: `ucs-detect`'s report of each terminal; without the queries of
  the terminal's features in iTerm2, which leaves one unanswered.
"""
import ctypes
import json
import os
import shutil
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
BENCH = os.path.join(ROOT, "target/release/lokyr-bench")
ONSCREEN = os.path.join(HERE, "onscreen.py")
PYTHON = "/usr/bin/python3"
KITTEN = "/Applications/kitty.app/Contents/MacOS/kitten"
VTEBENCH = "/tmp/lokyr/vtebench-bin/bin/vtebench"
VTEBENCH_DATA = os.path.join(ROOT, "target/vtebench/benchmarks")
UCS_DETECT = os.path.expanduser("~/.local/bin/ucs-detect")
CAT_FILE = "/tmp/lokyr/workloads/ascii.txt"
# Output that scrolls for some seconds, as macOS has no `timeout`.
FLOOD = "yes & flood=$!; sleep {seconds}; kill $flood"

XDG = "/tmp/lokyr/compare-xdg"
ZDOTDIR = "/tmp/lokyr/compare-zdotdir"
SAVED = "/tmp/lokyr/compare-saved"
WORK = "/tmp/lokyr/compare-work"

ITERM_PROFILE = "Lokyr Bench"
ITERM_GUID = "lokyr-bench-f2-134"
ITERM_DYNAMIC = os.path.expanduser(
    "~/Library/Application Support/iTerm2/DynamicProfiles/lokyr-bench.json"
)
TERMINAL_PROFILE = "Lokyr Bench"

# `LOKYR_APP` names another bundle of Lokyr, as an earlier commit's.
LOKYR_APP = os.environ.get("LOKYR_APP", os.path.join(ROOT, "target/Lokyr.app"))
ALACRITTY_APP = "/tmp/lokyr/alacritty/src/target/release/osx/Alacritty.app"

# Each terminal: its bundle, and its executable, by which its processes are
# found.
TERMINALS = [
    ("lokyr", LOKYR_APP, "lokyr"),
    ("ghostty", "/Applications/Ghostty.app", "ghostty"),
    ("alacritty", ALACRITTY_APP, "alacritty"),
    ("kitty", "/Applications/kitty.app", "kitty"),
    ("wezterm", "/Applications/WezTerm.app", "wezterm-gui"),
    ("iterm2", "/Applications/iTerm.app", "iTerm2"),
    ("terminal", "/System/Applications/Utilities/Terminal.app", "Terminal"),
]


def environment():
    """What every terminal finds in its environment: the configs, and a
    zsh that reads none of the user's files."""
    env = {
        "XDG_CONFIG_HOME": XDG,
        "ZDOTDIR": ZDOTDIR,
        "WEZTERM_CONFIG_FILE": os.path.join(XDG, "wezterm/wezterm.lua"),
    }
    # The Metal HUD's, while its log is taken.
    for name in ["MTL_HUD_ENABLED", "MTL_HUD_LOG_ENABLED"]:
        if name in os.environ:
            env[name] = os.environ[name]
    return env


def log(*parts):
    print(time.strftime("%H:%M:%S"), *parts, file=sys.stderr, flush=True)


def run(argv, **kwargs):
    return subprocess.run(argv, check=False, **kwargs)


# The setup, and the user's settings back.


def setup(configs):
    if os.path.exists(XDG):
        shutil.rmtree(XDG)
    for name in ["lokyr", "ghostty", "alacritty", "kitty", "wezterm"]:
        shutil.copytree(os.path.join(configs, name), os.path.join(XDG, name))
    os.makedirs(ZDOTDIR, exist_ok=True)
    os.makedirs(SAVED, exist_ok=True)
    # iTerm2: its dynamic profile, as the default, whose earlier default
    # is saved once.
    saved_guid = os.path.join(SAVED, "iterm2-default-guid")
    if not os.path.exists(saved_guid):
        guid = run(
            ["defaults", "read", "com.googlecode.iterm2", "Default Bookmark Guid"],
            capture_output=True,
            text=True,
        ).stdout.strip()
        with open(saved_guid, "w") as file:
            file.write(guid)
    os.makedirs(os.path.dirname(ITERM_DYNAMIC), exist_ok=True)
    shutil.copy(os.path.join(configs, "iterm2-profile.json"), ITERM_DYNAMIC)
    run(["defaults", "write", "com.googlecode.iterm2", "Default Bookmark Guid", ITERM_GUID])
    setup_terminal()


def setup_terminal():
    """A profile of Terminal.app, opaque, of Menlo 13 in 120 by 40 cells,
    with a steady cursor and 10 000 lines of scrollback, as its default;
    Terminal.app's settings saved first, once. Terminal.app quits for it,
    as its settings change while it does not run."""
    saved = os.path.join(SAVED, "com.apple.Terminal.plist")
    run(["osascript", "-e", 'quit app "Terminal"'])
    time.sleep(2)
    if not os.path.exists(saved):
        run(["defaults", "export", "com.apple.Terminal", saved])
    # The profile, made by Terminal.app itself, which archives its font.
    script = f'''
    tell application "Terminal"
        if not (exists settings set "{TERMINAL_PROFILE}") then
            make new settings set with properties {{name:"{TERMINAL_PROFILE}"}}
        end if
        set profile to settings set "{TERMINAL_PROFILE}"
        set font name of profile to "Menlo-Regular"
        set font size of profile to 13
        set number of columns of profile to 120
        set number of rows of profile to 40
        set background color of profile to {{0, 0, 0}}
        set normal text color of profile to {{65535, 65535, 65535}}
        set default settings to profile
        set startup settings to profile
        quit
    end tell
    '''
    run(["osascript", "-e", script])
    time.sleep(2)
    # The cursor and the scrollback, which AppleScript does not reach.
    current = os.path.join(WORK, "terminal.plist")
    os.makedirs(WORK, exist_ok=True)
    run(["defaults", "export", "com.apple.Terminal", current])
    edits = [
        ("CursorBlink", "bool", "false"),
        ("ShouldLimitScrollback", "integer", "1"),
        ("ScrollbackLines", "integer", "10000"),
        ("shellExitAction", "integer", "1"),
        ("BackgroundBlur", "real", "0"),
    ]
    for key, kind, value in edits:
        path = f":Window Settings:{TERMINAL_PROFILE}:{key}"
        if run(["/usr/libexec/PlistBuddy", "-c", f"Set '{path}' {value}", current],
               capture_output=True).returncode != 0:
            run(["/usr/libexec/PlistBuddy", "-c", f"Add '{path}' {kind} {value}", current])
    run(["defaults", "import", "com.apple.Terminal", current])


def restore():
    guid = os.path.join(SAVED, "iterm2-default-guid")
    if os.path.exists(guid):
        with open(guid) as file:
            value = file.read().strip()
        if value:
            run(["defaults", "write", "com.googlecode.iterm2", "Default Bookmark Guid", value])
    if os.path.exists(ITERM_DYNAMIC):
        os.remove(ITERM_DYNAMIC)
    terminal = os.path.join(SAVED, "com.apple.Terminal.plist")
    if os.path.exists(terminal):
        run(["osascript", "-e", 'quit app "Terminal"'])
        time.sleep(2)
        run(["defaults", "import", "com.apple.Terminal", terminal])
    log("restored iTerm2's and Terminal.app's settings")


# The processes of a terminal.


# Where iTerm2 keeps the servers of its sessions, outside its bundle, which
# outlive it: they count in its memory, and the user ends them.
ITERM_SERVERS = os.path.expanduser("~/Library/Application Support/iTerm2/iTermServer-")


def processes_of(app, servers=False):
    """The processes whose executables are in the bundle `app`, and, with
    `servers`, iTerm2's servers of its sessions."""
    out = run(["ps", "-A", "-o", "pid=,comm="], capture_output=True, text=True).stdout
    inside = os.path.realpath(app) + "/"
    pids = []
    for line in out.splitlines():
        pid, _, command = line.strip().partition(" ")
        command = command.strip()
        mine = command.startswith(inside) or command.startswith(app + "/")
        server = servers and app.endswith("iTerm.app") and command.startswith(ITERM_SERVERS)
        if mine or server:
            pids.append(int(pid))
    return pids


def energy_nj(pid):
    """The energy that `pid` has spent, in nanojoules, as the kernel bills
    it, `ri_energy_nj` of `rusage_info_v6`, the 41st counter after its
    UUID."""
    libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    buffer = (ctypes.c_uint64 * 60)()
    if libc.proc_pid_rusage(pid, 6, ctypes.byref(buffer)) != 0:
        return 0
    return buffer[2 + 40]


def quit_app(app):
    for pid in processes_of(app):
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    for _ in range(30):
        if not processes_of(app):
            return
        time.sleep(0.2)
    for pid in processes_of(app):
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    time.sleep(1)


def open_app(app):
    argv = ["open", "-n", "-a", app]
    for name, value in environment().items():
        argv += ["--env", f"{name}={value}"]
    run(argv)


# The measures.


def startup(out, chosen):
    env = [arg for name, value in environment().items() for arg in ("--env", f"{name}={value}")]
    for round_ in range(2):
        order = chosen[round_ % len(chosen):] + chosen[: round_ % len(chosen)]
        if round_ % 2:
            order.reverse()
        for name, app, _ in order:
            quit_app(app)
            result = os.path.join(out, f"startup-{name}-{round_ + 1}.json")
            log("startup", name, round_ + 1)
            with open(result, "w") as file:
                run([BENCH, "startup", "--app", app, "--runs", "5", "--warmup", "1", *env],
                    stdout=file, stderr=subprocess.DEVNULL)
            if name == "lokyr":
                result = os.path.join(out, f"startup-lokyr-first-frame-{round_ + 1}.json")
                with open(result, "w") as file:
                    run([BENCH, "startup", "--app", app, "--runs", "5", "--warmup", "1",
                         "--first-frame", *env], stdout=file, stderr=subprocess.DEVNULL)


def idle(out, chosen):
    for name, app, _ in chosen:
        quit_app(app)
        # iTerm2's servers of earlier sessions outlive it, and are not its
        # memory now.
        before = set(processes_of(app, servers=True))
        log("idle", name)
        open_app(app)
        time.sleep(10)
        pids = [pid for pid in processes_of(app, servers=True) if pid not in before]
        if not pids:
            log(name, "did not start")
            continue
        idles = [
            subprocess.Popen([BENCH, "idle", "--pid", str(pid), "--duration", "60"],
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            for pid in pids
        ]
        energy = subprocess.Popen(
            [BENCH, "energy", "--duration", "60", *[a for p in pids for a in ("--pid", str(p))]],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
        )
        reports = [json.loads(process.communicate()[0] or "null") for process in idles]
        energy_report = json.loads(energy.communicate()[0] or "null")
        memory = []
        for pid in pids:
            result = run([BENCH, "memory", "--pid", str(pid), "--duration", "5"],
                         capture_output=True, text=True)
            memory.append(json.loads(result.stdout or "null"))
        with open(os.path.join(out, f"idle-{name}.json"), "w") as file:
            json.dump({"pids": pids, "idle": reports, "energy": energy_report,
                       "memory": memory}, file, indent=1)
        quit_app(app)
        time.sleep(2)


def script_in(name, body):
    """A script of `body`, which every terminal runs alike."""
    os.makedirs(WORK, exist_ok=True)
    path = os.path.join(WORK, f"{name}.sh")
    with open(path, "w") as file:
        file.write("#!/bin/sh\n" + body)
    os.chmod(path, 0o755)
    return path


def launch_running(name, app, script):
    """`name` running `script` in a window of its own; the process that
    ends with the terminal, for those launched directly."""
    env = dict(os.environ, **environment())
    if name == "lokyr":
        argv = [os.path.join(app, "Contents/MacOS/lokyr"), "--bench", "output", "--", script]
    elif name == "ghostty":
        argv = [os.path.join(app, "Contents/MacOS/ghostty"), "-e", script]
    elif name == "alacritty":
        argv = [os.path.join(app, "Contents/MacOS/alacritty"), "-e", script]
    elif name == "kitty":
        argv = [os.path.join(app, "Contents/MacOS/kitty"), script]
    elif name == "wezterm":
        argv = [os.path.join(app, "Contents/MacOS/wezterm-gui"), "start", "--", script]
    elif name == "iterm2":
        open_app(app)
        time.sleep(3)
        run(["osascript", "-e",
             f'tell application "iTerm" to create window with profile "{ITERM_PROFILE}" '
             f'command "{script}"'])
        return None
    else:
        open_app(app)
        time.sleep(3)
        run(["osascript", "-e", f'tell application "Terminal" to do script "exec {script}"'])
        return None
    return subprocess.Popen(argv, env=env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)


def wait_for(path, timeout):
    start = time.time()
    while not os.path.exists(path) and time.time() - start < timeout:
        time.sleep(0.5)
    return os.path.exists(path)


def in_terminal(name, app, label, body, timeout):
    """Runs `body` in `name`, and waits for it to end, `timeout` at most."""
    quit_app(app)
    done = os.path.join(WORK, f"{label}.done")
    if os.path.exists(done):
        os.remove(done)
    script = script_in(label, body + f"\ntouch '{done}'\n")
    process = launch_running(name, app, script)
    ended = wait_for(done, timeout)
    if not ended:
        log(name, label, "did not end in", timeout, "s")
    if process is not None:
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
    quit_app(app)
    time.sleep(2)
    return ended


def check(out, chosen):
    for name, app, _ in chosen:
        log("check", name)
        result = os.path.join(out, f"size-{name}.txt")
        body = (f"stty size > '{result}'\n"
                f"echo \"$ZDOTDIR\" \"$XDG_CONFIG_HOME\" \"$TERM\" >> '{result}'\n"
                "sleep 2")
        in_terminal(name, app, f"check-{name}", body, 60)
        if os.path.exists(result):
            with open(result) as file:
                log(name, file.read().replace("\n", " | "))


def throughput(out, chosen):
    for name, app, _ in chosen:
        result = os.path.join(out, f"throughput-{name}")
        os.makedirs(result, exist_ok=True)
        log("throughput", name)
        cat = os.path.join(result, "cat.jsonl")
        kitten = name != "iterm2"
        body = "\n".join(
            [f"cd '{result}'"]
            + [f"'{KITTEN}' __benchmark__ > kitten-{n}.txt 2>&1" for n in range(1, 4) if kitten]
            + [f"'{KITTEN}' __benchmark__ --render > kitten-render-{n}.txt 2>&1"
               for n in range(1, 4) if kitten]
            # vtebench writes to the terminal, and its results to its file.
            + [f"'{VTEBENCH}' -b '{VTEBENCH_DATA}' --max-secs 5 --dat vtebench.dat --silent",
               f"{PYTHON} '{ONSCREEN}' '{CAT_FILE}' '{cat}' 5"]
        )
        in_terminal(name, app, f"throughput-{name}", body, 1800)


def energy(out, chosen):
    for name, app, _ in chosen:
        log("energy", name)
        quit_app(app)
        started = os.path.join(WORK, f"energy-{name}.started")
        if os.path.exists(started):
            os.remove(started)
        script = script_in(f"energy-{name}", f"touch '{started}'\n{FLOOD.format(seconds=20)}\n")
        process = launch_running(name, app, script)
        if not wait_for(started, 30):
            log(name, "did not start")
            quit_app(app)
            continue
        time.sleep(3)
        pids = processes_of(app)
        report = run([BENCH, "energy", "--duration", "10",
                      *[a for p in pids for a in ("--pid", str(p))]],
                     capture_output=True, text=True).stdout
        with open(os.path.join(out, f"energy-{name}.json"), "w") as file:
            file.write(report)
        if process is not None:
            process.terminate()
        quit_app(app)
        time.sleep(2)


def energycat(out, chosen):
    for name, app, _ in chosen:
        log("energycat", name)
        quit_app(app)
        started = os.path.join(WORK, f"energycat-{name}.started")
        done = os.path.join(WORK, f"energycat-{name}.done")
        result = os.path.join(out, f"energycat-{name}.jsonl")
        for path in [started, done, result]:
            if os.path.exists(path):
                os.remove(path)
        body = (f"touch '{started}'\nsleep 3\n"
                f"{PYTHON} '{ONSCREEN}' '{CAT_FILE}' '{result}' 1\n"
                f"touch '{done}'\nsleep 1\n")
        process = launch_running(name, app, script_in(f"energycat-{name}", body))
        if not wait_for(started, 30):
            log(name, "did not start")
            quit_app(app)
            continue
        time.sleep(1.5)
        pids = processes_of(app)
        before = {pid: energy_nj(pid) for pid in pids}
        ended = wait_for(done, 600)
        joules = sum(energy_nj(pid) - first for pid, first in before.items()) / 1e9
        with open(os.path.join(out, f"energycat-{name}.json"), "w") as file:
            json.dump({"pids": pids, "joules": joules, "ended": ended}, file)
        if process is not None:
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
        quit_app(app)
        time.sleep(2)


def hud(out, chosen):
    for name, app, _ in chosen:
        log("hud", name)
        quit_app(app)
        stream = subprocess.Popen(
            ["log", "stream", "--style", "compact", "--predicate",
             'eventMessage CONTAINS "metal-HUD"'],
            stdout=open(os.path.join(out, f"hud-{name}.log"), "w"),
            stderr=subprocess.DEVNULL,
        )
        os.environ["MTL_HUD_ENABLED"] = "1"
        os.environ["MTL_HUD_LOG_ENABLED"] = "1"
        try:
            in_terminal(name, app, f"hud-{name}", FLOOD.format(seconds=10), 60)
        finally:
            del os.environ["MTL_HUD_ENABLED"]
            del os.environ["MTL_HUD_LOG_ENABLED"]
            stream.terminate()


def ucs(out, chosen):
    for name, app, _ in chosen:
        log("ucs", name)
        report = os.path.join(out, f"ucs-{name}.yaml")
        # It talks with the terminal through its output, and its errors go
        # to a file.
        features = " --no-terminal-test" if name == "iterm2" else ""
        body = (f"'{UCS_DETECT}' --save-yaml '{report}' --limit-category-time 10"
                f" --set-software-name {name} --no-final-summary --probe-silently{features}"
                f" --stream stdout 2> '{WORK}/ucs-{name}.err'")
        in_terminal(name, app, f"ucs-{name}", body, 1800)


def main():
    measure = sys.argv[1]
    if measure == "restore":
        restore()
        return
    configs, out = sys.argv[2], sys.argv[3]
    names = sys.argv[4].split(",") if len(sys.argv) > 4 else None
    chosen = [t for t in TERMINALS if names is None or t[0] in names]
    os.makedirs(out, exist_ok=True)
    if measure == "setup":
        setup(configs)
        return
    {
        "check": check,
        "startup": startup,
        "idle": idle,
        "throughput": throughput,
        "energy": energy,
        "energycat": energycat,
        "hud": hud,
        "ucs": ucs,
    }[measure](out, chosen)
    log("done", measure)


if __name__ == "__main__":
    main()
