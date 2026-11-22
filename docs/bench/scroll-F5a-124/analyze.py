"""Tables of the scroll at 120 Hz, from a session's reports: each way's
frames that the app presented a second and the refreshes that had one, the
CPU, and the idle after; then each scroll by hand, of the log of 1.4.0's
copy of temporary logs: the refreshes while the view moved and how many had
a frame drawn, and the events' gaps.

    python3 analyze.py [<reports' directory>] [<refresh's rate, Hz>]
"""
import collections, json, os, re, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/scroll-F5a-124"
HZ = float(sys.argv[2]) if len(sys.argv) > 2 else 120.0


def load(name):
    path = os.path.join(OUT, name)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def scroll():
    for v, way in (("v130", "rows"), ("v140", "rows"), ("v140", "pixels")):
        rows = []
        for r in (1, 2):
            d = load(f"{v}-{way}-{r}.figures.json")
            i = load(f"idle-{v}-{way}-{r}.json")
            if d:
                rows.append((r, d, i))
        if not rows:
            continue
        for r, d, i in rows:
            idle = (f"; then idle, package {i['summary']['package_idle_wakeups_per_s']:.3f}/s, "
                    f"interrupt {i['summary']['interrupt_wakeups_per_s']:.3f}/s" if i else "")
            print(f"{v} {way} round {r}: app {d.get('app_frames_per_second')} frames/s, refreshes with a "
                  f"frame {d.get('refreshes_with_a_frame')}, gaps {d.get('app_frame_gaps')}; display "
                  f"{d['frames_per_second']}/s, missed {d['missed']}; window server {d['windowserver_cpu']}%, "
                  f"app {d['app_cpu']}% ({d['seconds']} s, {d['refresh_ms']} ms){idle}")
        fps = [d.get("app_frames_per_second", 0) for _, d, _ in rows]
        share = [d.get("refreshes_with_a_frame", 0) for _, d, _ in rows]
        app = [d["app_cpu"] for _, d, _ in rows]
        ws = [d["windowserver_cpu"] for _, d, _ in rows]
        print(f"{v} {way}: app frames mean {st.mean(fps):.1f}/s, refreshes with a frame mean {st.mean(share):.3f}, "
              f"app CPU mean {st.mean(app):.1f}%, window server's {st.mean(ws):.1f}%")
        print()


def hand(path):
    ev = []
    for line in open(path):
        m = re.match(r"LT (\d+) (.*)", line)
        if m:
            ev.append((int(m.group(1)), m.group(2)))
    moves = [t for t, s in ev if s.startswith("scroll_by") and s.endswith("moved=true")]
    draws = [t for t, s in ev if s == "draw same=false"]
    refresh = 1e6 / HZ
    # The view's moving stretches: its moves less than 50 ms apart.
    stretches, start, last = [], None, None
    for t in moves:
        if start is None or t - last > 50e3:
            if start is not None:
                stretches.append((start, last))
            start = t
        last = t
    if start is not None:
        stretches.append((start, last))
    total = drawn = 0
    for a, b in stretches:
        n = int((b - a) // refresh) + 1
        bins = {int((t - a) // refresh) for t in draws if a <= t <= b + refresh}
        total += n
        drawn += min(len(bins), n)
    print(f"{os.path.basename(path)}: {len(stretches)} stretches moving, {total / HZ:.1f} s; "
          f"{total} refreshes, {drawn} of a frame ({100 * drawn / max(total, 1):.0f} %)")
    scrolls = [t for t, s in ev if s.startswith("scroll evts")]
    gaps = [(b - a) / 1000 for a, b in zip(scrolls, scrolls[1:]) if b - a < 50e3]
    print("  the events' gaps, ms:", dict(sorted(collections.Counter(round(g) for g in gaps).items())))
    momentum = collections.Counter(re.search(r"momentum=(\S+)", s).group(1) for t, s in ev if s.startswith("scroll evts"))
    print("  events of the momentum:", dict(momentum))
    draw_gaps = []
    for a, b in stretches:
        inside = [t for t in draws if a <= t <= b + refresh]
        draw_gaps += [(y - x) / refresh for x, y in zip(inside, inside[1:])]
    print("  the gaps between frames, in refreshes:",
          dict(sorted(collections.Counter(round(g) for g in draw_gaps).items())))


print(f"== the scroll at {HZ:g} Hz")
scroll()
logs = sorted(f for f in os.listdir(OUT) if re.match(r"hand-\d+\.log$", f)) if os.path.isdir(OUT) else []
if logs:
    print("== by hand")
    for f in logs:
        hand(os.path.join(OUT, f))
