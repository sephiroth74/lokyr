"""Tables of the A/B of F3-29, from the session's reports."""
import json, os, statistics as st
OUT = "/tmp/lokyr/bench-F3-29"
def load(name):
    path = os.path.join(OUT, name)
    return json.load(open(path)) if os.path.exists(path) else None
print("## Startup, warm, first frame (ms): median, spread; floor's first window; gap")
for r in (1, 2):
    floor = load(f"startup-floor-{r}.json")
    fm = floor["summary"]["from_start_ms"]["median"] if floor else None
    for v in ("v030", "v040"):
        d = load(f"startup-{v}-{r}.json")
        if not d: continue
        s = d["summary"]; m = s["from_start_ms"]["median"]
        print(f"round {r} {v}: {m:.1f} ms, p90 {s['from_start_ms']['p90']:.1f}, spread {s['spread']*100:.1f}%; floor {fm:.1f}; gap {m-fm:.1f}")
print("## Latency (ms)")
for r in (1, 2):
    for v in ("v030", "v040"):
        d = load(f"latency-{v}-{r}.json")
        if not d: continue
        l = d["latency_ms"]; f = d["frames"]
        print(f"round {r} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} p99 {l['p99']:.2f} max {l['max']:.2f}; frames p50 {f['p50']:.2f} p99 {f['p99']:.2f}; refresh {d['refresh_ms']}")
print("## Idle (per s) and memory (MB)")
for r in (1, 2):
    for v in ("v030", "v040"):
        i = load(f"idle-{v}-{r}.json"); m = load(f"memory-{v}-{r}.json"); m0 = load(f"memory-start-{v}-{r}.json")
        if not i or not m: continue
        s = i["summary"]; ms = m["summary"]
        peak = m0["summary"].get("lifetime_peak") if m0 else None
        print(f"round {r} {v}: package idle {s['package_idle_wakeups_per_s']:.3f}/s, interrupt {s['interrupt_wakeups_per_s']:.3f}/s; "
              f"footprint w/o GPU {ms['footprint_without_gpu']['median']/1e6:.1f} MB, GPU {ms['gpu']['median']/1e6:.1f} MB, lifetime peak {ms.get('lifetime_peak',0)/1e6:.1f} MB")
print("## Throughput on screen (MiB/s, median of 3) and frames (ms)")
for f in ("ascii", "seq", "sgr", "cjk"):
    for v in ("v030", "v040"):
        path = os.path.join(OUT, f"onscreen-{f}-{v}.jsonl")
        if not os.path.exists(path): continue
        runs = [json.loads(l) for l in open(path) if l.strip()]
        rates = [x["mib_s"] for x in runs]
        fr = load(f"frames-{f}-{v}.json")
        frame = f"frames {fr['frames']}, total p50 {fr['total_ms']['p50']:.3f} p99 {fr['total_ms']['p99']:.3f} max {fr['total_ms']['max']:.3f}" if fr else ""
        print(f"{f} {v}: {st.median(rates):.1f} MiB/s (runs {', '.join(f'{x:.1f}' for x in rates)}); {frame}")
