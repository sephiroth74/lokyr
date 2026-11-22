"""Of a log of a scroll by hand, of the copy of temporary logs: the gaps
between the frames drawn, in refreshes, and the share of the refreshes
with a frame, by the scroll's speed then, the points of its events over
the last 50 ms a 60th of a second.

    python3 bands.py <hand log> [<refresh's rate, Hz>]
"""
import re, collections, bisect, sys
ev = []
for line in open(sys.argv[1]):
    m = re.match(r"LT (\d+) (.*)", line)
    if m: ev.append((int(m.group(1)), m.group(2)))
R = 1e6 / float(sys.argv[2]) if len(sys.argv) > 2 else 1e6 / 60
scroll = [(t, abs(float(re.search(r"dy=(\S+)", s).group(1)))) for t, s in ev if s.startswith("scroll evts")]
st = [t for t, _ in scroll]
draws = sorted(t for t, s in ev if s == "draw same=false")
def speed(t):
    i = bisect.bisect_right(st, t); j = bisect.bisect_left(st, t - 50e3)
    return sum(d for _, d in scroll[j:i]) / 3.0
res = collections.defaultdict(collections.Counter)
for a, b in zip(draws, draws[1:]):
    g = round((b - a) / R)
    if g > 6: continue
    v = speed(b)
    band = "fast >8" if v > 8 else ("medium 2-8" if v > 2 else "slow <2")
    res[band][g] += 1
for band in ("fast >8", "medium 2-8", "slow <2"):
    c = res[band]; n = sum(c.values()); refreshes = sum(k * v for k, v in c.items() if k > 0)
    print(f"{band:11s}: draw gaps in refreshes {dict(sorted(c.items()))}; refreshes with a frame {100*(n - c[0])/max(refreshes,1):.0f}%")
