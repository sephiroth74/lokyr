"""Tables of startup-F5b-45's session: the App Launch traces' periods of
each copy, then the rounds' startups and their gaps to the minimal app, of
Lokyr's registrations as they were ("few"), of 20 more ("many"), and of
those taken away again ("again").

    python3 analyze.py [<the session's directory>]
"""
import json, os, statistics as st, sys
import xml.etree.ElementTree as ET

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../target/bench/startup-F5b-45")
STATES = {"few": 5, "many": 5, "again": 4}
TRACED = ("few", "many")
COPIES = ("v140", "v150")
ONGOING = 2**64 - 1


def rows(path, process="lokyr"):
    """The rows of an exported table of `process`, each a dict of its
    columns' mnemonics to their elements, refs resolved."""
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return []
    root = ET.parse(path).getroot()
    schema = root.find(".//schema")
    if schema is None:
        return []
    names = [col.findtext("mnemonic") for col in schema.findall("col")]
    seen = {}

    def resolve(e):
        if "ref" in e.attrib:
            return seen[e.attrib["ref"]]
        for d in e.iter():
            if "id" in d.attrib:
                seen[d.attrib["id"]] = d
        return e

    out = []
    for row in root.iter("row"):
        cells = [resolve(c) for c in row]
        values = dict(zip(names, cells))
        proc = values.get("process")
        if proc is not None and not proc.attrib.get("fmt", "").startswith(process + " "):
            continue
        out.append(values)
    return out


def ms(e):
    return int(e.text) / 1e6


def trace(state, v, i):
    """The launch's start, each period's start and duration of a trace."""
    base = f"{OUT}/trace-{state}-{v}-{i}"
    launch = None
    for r in rows(f"{base}-dyld-activity-interval.xml"):
        if r.get("type") is not None and r["type"].text == "Launch Executable":
            launch = ms(r["start"])
            break
    found = []
    for r in rows(f"{base}-life-cycle-period.xml"):
        name = r["period"].attrib.get("fmt")
        if any(name == n for n, _, _ in found):
            continue
        duration = int(r["duration"].text)
        found.append((name, ms(r["start"]), None if duration == ONGOING else duration / 1e6))
    # A period still going at the trace's end, or of no end told, lasts
    # until the next one starts.
    periods = {}
    for k, (name, start, duration) in enumerate(found):
        if duration is None and k + 1 < len(found):
            duration = found[k + 1][1] - start
        periods[name] = (start, duration)
    return launch, periods


def mean_sd(values):
    return f"{st.mean(values):.1f} (sd {st.pstdev(values):.1f})" if values else "-"


print("== the App Launch traces: each period's duration, ms, and its start from dyld's launch")
by_copy = {(state, v): {} for state in TRACED for v in COPIES}
for state, v in by_copy:
    for i in (1, 2):
        launch, periods = trace(state, v, i)
        if not periods:
            continue
        parts = []
        for name, (start, duration) in periods.items():
            at = f"@{start - launch:.1f}" if launch is not None else ""
            d = f"{duration:.1f}" if duration is not None else "…"
            parts.append(f"{name.replace('Initializing - ', '').replace('Launching - ', '')} {d}{at}")
            by_copy[(state, v)].setdefault(name, {"start": [], "duration": []})
            if launch is not None:
                by_copy[(state, v)][name]["start"].append(start - launch)
            if duration is not None:
                by_copy[(state, v)][name]["duration"].append(duration)
        print(f"{state} {v} {i}: " + "; ".join(parts))
print()
for (state, v), names in by_copy.items():
    for name, m in names.items():
        print(f"{state} {v} {name}: duration {mean_sd(m['duration'])}, start {mean_sd(m['start'])}")
print()


def load(name):
    path = os.path.join(OUT, name)
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


summary = {}
for label, count in STATES.items():
    medians = {v: {} for v in COPIES}
    gaps = {v: [] for v in COPIES}
    floors = []
    print(f"== startup {label}, warm, first frame, 10 runs after 2")
    for r in range(1, count + 1):
        fl = load(f"startup-{label}-floor-{r}.json")
        fm = fl["summary"]["from_start_ms"]["median"] if fl else None
        if fm:
            floors.append(fm)
        line = []
        for v in COPIES:
            d = load(f"startup-{label}-{v}-{r}.json")
            if not d:
                continue
            m = d["summary"]["from_start_ms"]["median"]
            medians[v][r] = m
            if fm:
                gaps[v].append(m - fm)
            line.append(f"{v} {m:.1f}")
        if line:
            print(f"round {r}: {'; '.join(line)}; " + (f"floor {fm:.1f}" if fm else "no floor"))
    if floors:
        print(f"floor: mean {mean_sd(floors)}, from {min(floors):.1f} to {max(floors):.1f}")
    for v in COPIES:
        values = list(medians[v].values())
        if values:
            print(f"{v}: mean of rounds {mean_sd(values)}; gap mean {mean_sd(gaps[v])}")
            summary[(label, v)] = (st.mean(values), st.mean(gaps[v]) if gaps[v] else None)
    if floors:
        summary[(label, "floor")] = (st.mean(floors), None)
    print()

for later in ("many", "again"):
    print(f"== {later} − few")
    for v in COPIES + ("floor",):
        b, a = summary.get(("few", v)), summary.get((later, v))
        if b and a:
            gap = f", gap {a[1] - b[1]:+.1f}" if a[1] is not None and b[1] is not None else ""
            print(f"{v}: startup {a[0] - b[0]:+.1f}{gap}")
