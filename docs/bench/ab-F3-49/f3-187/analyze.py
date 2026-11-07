"""The startup and the latency of F3-187's session: the build before the
fix (cur), the fix with the tabs plugin (fix) and without it (fixnotabs), of
the medians and the means of the four rounds."""
import json
import pathlib

HERE = pathlib.Path(__file__).parent
VARIANTS = ["cur", "fix", "fixnotabs"]

for kind, label in [("first", "first frame"), ("window", "window on screen")]:
    for v in VARIANTS:
        medians = [
            json.loads((HERE / f"{kind}-{v}-{r}.json").read_text())["summary"]["from_start_ms"]["median"]
            for r in range(1, 5)
        ]
        print(f"startup, {label:16} {v:10} {medians} mean {sum(medians) / 4:.1f} ms")
for v in VARIANTS:
    p50, p99, first, late = [], [], [], 0
    for r in range(1, 5):
        d = json.loads((HERE / f"latency-{v}-{r}.json").read_text())
        p50.append(d["latency_ms"]["p50"])
        p99.append(d["latency_ms"]["p99"])
        first.append(d["histogram"].get("1", 0))
        late += sum(n for k, n in d["histogram"].items() if int(k) >= 4)
    print(
        f"latency {v:10} p50 {sum(p50) / 4:.2f} ms, p99 {sum(p99) / 4:.2f} ms, "
        f"first refresh {first}, fourth or later {late} of {sum(json.loads((HERE / f'latency-{v}-{r}.json').read_text())['samples'] for r in range(1, 5))}"
    )
