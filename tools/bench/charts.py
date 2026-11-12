"""Charts of the benches' reports, as SVG files that a Markdown page shows:
bars of one value each, of which one stands out, and pairs of a value
before and after a change. Each chart follows the reader's light or dark
appearance, and tells each mark's value as the pointer rests on it, where
the browser shows the SVG itself.

The colors: the accent blue for what the chart is about, a gray for the
rest, which recedes, and an orange for a second series beside the blue;
text, grid and axes in their own grays.
"""
from html import escape
import math

STYLE = """
<style>
  .surface { fill: #fcfcfb; }
  .primary { fill: #0b0b0b; }
  .secondary { fill: #52514e; }
  .muted { fill: #898781; }
  .grid { stroke: #e1e0d9; }
  .base { stroke: #c3c2b7; }
  .accent { fill: #2a78d6; }
  .context { fill: #898781; }
  .accent-line { stroke: #2a78d6; }
  .context-line { stroke: #898781; }
  .second { fill: #eb6834; }
  .second-line { stroke: #eb6834; }
  .ring { stroke: #fcfcfb; }
  .budget { stroke: #52514e; }
  text { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
  .tick { font-variant-numeric: tabular-nums; }
  @media (prefers-color-scheme: dark) {
    .surface { fill: #1a1a19; }
    .primary { fill: #f0efec; }
    .secondary { fill: #c3c2b7; }
    .muted { fill: #898781; }
    .grid { stroke: #2c2c2a; }
    .base { stroke: #383835; }
    .accent { fill: #3987e5; }
    .context { fill: #6f6e69; }
    .accent-line { stroke: #3987e5; }
    .context-line { stroke: #6f6e69; }
    .second { fill: #d95926; }
    .second-line { stroke: #d95926; }
    .ring { stroke: #1a1a19; }
    .budget { stroke: #c3c2b7; }
  }
</style>
"""

WIDTH = 680
LEFT = 118
RIGHT = 70
TOP = 62
BAND = 30
BAR = 18


def nice_ticks(top, count=5):
    """Round ticks from 0 past `top`."""
    if top <= 0:
        return [0, 1]
    raw = top / count
    power = 10 ** math.floor(math.log10(raw))
    step = min((s for s in (1, 2, 2.5, 5, 10) if s * power >= raw), default=10) * power
    ticks = [0.0]
    while ticks[-1] < top:
        ticks.append(round(ticks[-1] + step, 10))
    return ticks


def number(value, digits):
    """`value` with `digits` decimals, its halves rounded away from
    zero."""
    scaled = math.floor(abs(value) * 10 ** digits + 0.5) / 10 ** digits
    return f"{math.copysign(scaled, value):,.{digits}f}"


def compact(value):
    if value >= 10000:
        return f"{value / 1000:.0f}K"
    if value >= 1000:
        return f"{value / 1000:.1f}K".replace(".0K", "K")
    return f"{value:g}"


# A subtitle's line, and its height, which a longer subtitle wraps at.
SUBTITLE_SIZE = 12
SUBTITLE_LINE = 16


def text_width(text, size):
    """About the width of `text` at `size` points in the system's sans
    serif, of its characters' kinds: no font's metrics, but wide enough
    that a line of it fits where this says it does."""
    width = 0.0
    for char in text:
        if char in "iljI.,:;'|!()[] ":
            width += 0.29
        elif char in "frt":
            width += 0.36
        elif char in "mwMW":
            width += 0.82
        elif char.isupper() or char in "×%&@":
            width += 0.66
        elif char.isdigit():
            width += 0.57
        else:
            width += 0.53
    return width * size


def subtitle_lines(subtitle):
    """`subtitle` in lines that each fit the chart's width, broken at its
    spaces, of a margin, as the fonts of other systems than the Mac's, which
    GitHub's pages take, run wider."""
    lines, line = [], ""
    for word in subtitle.split(" "):
        wider = f"{line} {word}" if line else word
        if line and text_width(wider, SUBTITLE_SIZE) * 1.1 > WIDTH - 40:
            lines.append(line)
            line = word
        else:
            line = wider
    return lines + [line]


def top_of(subtitle):
    """Where a chart's content starts under its title and `subtitle`: `TOP`
    for a subtitle of one line, a line lower for each more."""
    return TOP + SUBTITLE_LINE * (len(subtitle_lines(subtitle)) - 1)


def header(title, subtitle, height):
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" '
        f'viewBox="0 0 {WIDTH} {height}" role="img" aria-label="{escape(title)}">',
        STYLE,
        f'<rect class="surface" width="{WIDTH}" height="{height}" rx="8"/>',
        f'<text class="primary" x="16" y="26" font-size="15" font-weight="600">{escape(title)}</text>',
    ]
    for index, line in enumerate(subtitle_lines(subtitle)):
        parts.append(f'<text class="secondary" x="16" y="{45 + SUBTITLE_LINE * index}" '
                     f'font-size="{SUBTITLE_SIZE}">{escape(line)}</text>')
    return parts


def band_of(rows, room, height):
    """The rows' band: `BAND`, or, for a chart of `height`, the band that
    fills its `room` of rows, as charts set side by side take one height."""
    if height is None:
        return BAND
    band = room / rows
    if band < BAND:
        raise ValueError(f"{rows} rows take more than the height of {height}")
    return band


def axis(parts, ticks, scale, bottom, top=TOP):
    for tick in ticks:
        x = LEFT + scale(tick)
        parts.append(f'<line class="grid" x1="{x:.1f}" y1="{top - 6}" x2="{x:.1f}" '
                     f'y2="{bottom}" stroke-width="1"/>')
        parts.append(f'<text class="muted tick" x="{x:.1f}" y="{bottom + 15}" font-size="11" '
                     f'text-anchor="middle">{compact(tick)}</text>')
    parts.append(f'<line class="base" x1="{LEFT}" y1="{top - 6}" x2="{LEFT}" y2="{bottom}" '
                 'stroke-width="1"/>')


def bar_path(x, y, length):
    """A bar from the baseline at `x`, rounded at its data end only."""
    r = min(4, length / 2, BAR / 2)
    return (f"M{x:.1f},{y:.1f} h{max(length - r, 0):.1f} "
            f"a{r},{r} 0 0 1 {r},{r} v{BAR - 2 * r:.1f} a{r},{r} 0 0 1 {-r},{r} "
            f"h{-max(length - r, 0):.1f} z")


def bars(title, subtitle, rows, unit, digits=0, order="ascending", height=None):
    """Bars of `rows`, `(label, value, stands_out)`, each labelled with its
    value in `unit`, sorted as `order` says, the best first; of `height`,
    if given, the rows spread to fill it."""
    rows = sorted(rows, key=lambda row: row[1], reverse=order == "descending")
    top = top_of(subtitle)
    band = band_of(len(rows), (height or 0) - top - 34, height)
    height = height or top + band * len(rows) + 34
    bottom = top + band * len(rows) - 6
    ticks = nice_ticks(max(row[1] for row in rows))
    span = WIDTH - LEFT - RIGHT
    scale = lambda value: value / ticks[-1] * span
    parts = header(title, subtitle, height)
    axis(parts, ticks, scale, bottom, top)
    for index, (label, value, stands_out) in enumerate(rows):
        y = top + index * band + (band - BAR) / 2 - 6
        weight = ' font-weight="600"' if stands_out else ""
        ink = "primary" if stands_out else "secondary"
        text = number(value, digits)
        parts.append(f'<text class="{ink}" x="{LEFT - 10}" y="{y + BAR / 2 + 4:.1f}" '
                     f'font-size="12" text-anchor="end"{weight}>{escape(label)}</text>')
        parts.append(f'<path class="{"accent" if stands_out else "context"}" '
                     f'd="{bar_path(LEFT, y, max(scale(value), 1))}">'
                     f'<title>{escape(label)}: {text} {escape(unit)}</title></path>')
        parts.append(f'<text class="{ink} tick" x="{LEFT + scale(value) + 6:.1f}" '
                     f'y="{y + BAR / 2 + 4:.1f}" font-size="12"{weight}>{text}</text>')
    parts.append("</svg>")
    return "\n".join(parts)


def before_after(title, subtitle, rows, unit, labels=("before", "after"), digits=0,
                 budgets=None):
    """For each of `rows`, `(label, before, after)`, a gray dot of the value
    before, a blue one of the value after, and a line between them, with a
    legend of `labels`; and, with `budgets`, a value for each row or None, a
    tick of the row's budget."""
    base = top_of(subtitle)
    height = base + 22 + BAND * len(rows) + 34
    top = base + 22
    bottom = top + BAND * len(rows) - 6
    highest = max(max(row[1], row[2]) for row in rows)
    if budgets:
        highest = max([highest] + [b for b in budgets if b is not None])
    ticks = nice_ticks(highest)
    span = WIDTH - LEFT - RIGHT
    scale = lambda value: value / ticks[-1] * span
    parts = header(title, subtitle, height)
    # The legend, a dot of each beside its name.
    parts.append(f'<circle class="context" cx="22" cy="{base + 2}" r="5"/>')
    parts.append(f'<text class="secondary" x="32" y="{base + 6}" font-size="12">{escape(labels[0])}</text>')
    offset = 44 + 7 * len(labels[0])
    parts.append(f'<circle class="accent" cx="{offset}" cy="{base + 2}" r="5"/>')
    parts.append(f'<text class="secondary" x="{offset + 10}" y="{base + 6}" font-size="12">'
                 f'{escape(labels[1])}</text>')
    if budgets:
        offset += 22 + 7 * len(labels[1])
        parts.append(f'<line class="budget" x1="{offset}" y1="{base - 5}" x2="{offset}" '
                     f'y2="{base + 9}" stroke-width="2"/>')
        parts.append(f'<text class="secondary" x="{offset + 8}" y="{base + 6}" font-size="12">'
                     'budget</text>')
    for tick in ticks:
        x = LEFT + scale(tick)
        parts.append(f'<line class="grid" x1="{x:.1f}" y1="{top - 6}" x2="{x:.1f}" '
                     f'y2="{bottom}" stroke-width="1"/>')
        parts.append(f'<text class="muted tick" x="{x:.1f}" y="{bottom + 15}" font-size="11" '
                     f'text-anchor="middle">{compact(tick)}</text>')
    parts.append(f'<line class="base" x1="{LEFT}" y1="{top - 6}" x2="{LEFT}" y2="{bottom}" '
                 'stroke-width="1"/>')
    for index, (label, before, after) in enumerate(rows):
        y = top + index * BAND + BAND / 2 - 6
        xb, xa = LEFT + scale(before), LEFT + scale(after)
        parts.append(f'<text class="secondary" x="{LEFT - 10}" y="{y + 4:.1f}" font-size="12" '
                     f'text-anchor="end">{escape(label)}</text>')
        if budgets and budgets[index] is not None:
            xt = LEFT + scale(budgets[index])
            parts.append(f'<line class="budget" x1="{xt:.1f}" y1="{y - 9:.1f}" x2="{xt:.1f}" '
                         f'y2="{y + 9:.1f}" stroke-width="2"><title>{escape(label)}, budget: '
                         f'{number(budgets[index], digits)} {escape(unit)}</title></line>')
        parts.append(f'<line class="context-line" x1="{xb:.1f}" y1="{y:.1f}" x2="{xa:.1f}" '
                     f'y2="{y:.1f}" stroke-width="2" stroke-linecap="round"/>')
        parts.append(f'<circle class="context ring" cx="{xb:.1f}" cy="{y:.1f}" r="5" '
                     f'stroke-width="2"><title>{escape(label)}, {escape(labels[0])}: '
                     f'{number(before, digits)} {escape(unit)}</title></circle>')
        parts.append(f'<circle class="accent ring" cx="{xa:.1f}" cy="{y:.1f}" r="5" '
                     f'stroke-width="2"><title>{escape(label)}, {escape(labels[1])}: '
                     f'{number(after, digits)} {escape(unit)}</title></circle>')
        # The value after beside its dot, on the side away from the line.
        if after < before:
            x, anchor = xa - 10, "end"
        else:
            x, anchor = xa + 10, "start"
        parts.append(f'<text class="primary tick" x="{x:.1f}" y="{y + 4:.1f}" font-size="12" '
                     f'font-weight="600" text-anchor="{anchor}">{number(after, digits)}</text>')
    parts.append("</svg>")
    return "\n".join(parts)


def paired_columns(title, subtitle, categories, before, after, unit, labels=("before", "after")):
    """Columns of two series over `categories`, the one before in gray and
    the one after in blue, side by side, each with its value above it."""
    base = top_of(subtitle)
    height = 300
    top, bottom = base + 34, height - 44
    left, right = 52, 24
    ticks = nice_ticks(max(max(before), max(after)), 4)
    scale = lambda value: value / ticks[-1] * (bottom - top)
    parts = header(title, subtitle, height)
    parts.append(f'<rect class="context" x="16" y="{base - 4}" width="12" height="12" rx="2"/>')
    parts.append(f'<text class="secondary" x="34" y="{base + 6}" font-size="12">{escape(labels[0])}</text>')
    offset = 50 + 7 * len(labels[0])
    parts.append(f'<rect class="accent" x="{offset}" y="{base - 4}" width="12" height="12" rx="2"/>')
    parts.append(f'<text class="secondary" x="{offset + 18}" y="{base + 6}" font-size="12">'
                 f'{escape(labels[1])}</text>')
    for tick in ticks:
        y = bottom - scale(tick)
        parts.append(f'<line class="grid" x1="{left}" y1="{y:.1f}" x2="{WIDTH - right}" '
                     f'y2="{y:.1f}" stroke-width="1"/>')
        parts.append(f'<text class="muted tick" x="{left - 8}" y="{y + 4:.1f}" font-size="11" '
                     f'text-anchor="end">{compact(tick)}</text>')
    band = (WIDTH - left - right) / len(categories)
    width = min(BAR * 2, band / 3)
    for index, category in enumerate(categories):
        middle = left + band * (index + 0.5)
        for value, x, style, name in ((before[index], middle - width - 1, "context", labels[0]),
                                      (after[index], middle + 1, "accent", labels[1])):
            h = scale(value)
            if value:
                r = min(4, h / 2, width / 2)
                parts.append(
                    f'<path class="{style}" d="M{x:.1f},{bottom} v{-(h - r):.1f} '
                    f'a{r},{r} 0 0 1 {r},{-r} h{width - 2 * r:.1f} a{r},{r} 0 0 1 {r},{r} '
                    f'v{h - r:.1f} z"><title>{escape(str(category))}, {escape(name)}: '
                    f'{value:g} {escape(unit)}</title></path>')
            parts.append(f'<text class="secondary tick" x="{x + width / 2:.1f}" y="{bottom - h - 5:.1f}" '
                         f'font-size="11" text-anchor="middle">{value:g}</text>')
        parts.append(f'<text class="secondary" x="{middle:.1f}" y="{bottom + 18}" font-size="12" '
                     f'text-anchor="middle">{escape(str(category))}</text>')
    parts.append(f'<line class="base" x1="{left}" y1="{bottom}" x2="{WIDTH - right}" y2="{bottom}" '
                 'stroke-width="1"/>')
    parts.append("</svg>")
    return "\n".join(parts)


def ranges(title, subtitle, rows, unit, labels=("median", "99th percentile"), digits=1,
           height=None):
    """For each of `rows`, `(label, low, high, stands_out)`, a line from a
    filled dot of `low` to a ring of `high`, each with its value beside it,
    the row that stands out in blue and the rest in gray, sorted by `high`,
    the best first; with a legend of `labels`; of `height`, if given, the
    rows spread to fill it."""
    rows = sorted(rows, key=lambda row: row[2])
    legend = top_of(subtitle)
    top = legend + 22
    band = band_of(len(rows), (height or 0) - top - 34, height)
    height = height or top + band * len(rows) + 34
    bottom = top + band * len(rows) - 6
    ticks = nice_ticks(max(row[2] for row in rows))
    span = WIDTH - LEFT - RIGHT
    scale = lambda value: value / ticks[-1] * span
    parts = header(title, subtitle, height)
    # The legend, of the dots' shapes, which tell the two values apart
    # without their color.
    parts.append(f'<circle class="context" cx="22" cy="{legend + 2}" r="5"/>')
    parts.append(f'<text class="secondary" x="32" y="{legend + 6}" font-size="12">{escape(labels[0])}</text>')
    offset = 44 + 7 * len(labels[0])
    parts.append(f'<circle class="surface context-line" cx="{offset}" cy="{legend + 2}" r="4.5" '
                 'stroke-width="2"/>')
    parts.append(f'<text class="secondary" x="{offset + 10}" y="{legend + 6}" font-size="12">'
                 f'{escape(labels[1])}</text>')
    for tick in ticks:
        x = LEFT + scale(tick)
        parts.append(f'<line class="grid" x1="{x:.1f}" y1="{top - 6}" x2="{x:.1f}" '
                     f'y2="{bottom}" stroke-width="1"/>')
        parts.append(f'<text class="muted tick" x="{x:.1f}" y="{bottom + 15}" font-size="11" '
                     f'text-anchor="middle">{compact(tick)}</text>')
    parts.append(f'<line class="base" x1="{LEFT}" y1="{top - 6}" x2="{LEFT}" y2="{bottom}" '
                 'stroke-width="1"/>')
    for index, (label, low, high, stands_out) in enumerate(rows):
        y = top + index * band + band / 2 - 6
        xl, xh = LEFT + scale(low), LEFT + scale(high)
        tone = "accent" if stands_out else "context"
        weight = ' font-weight="600"' if stands_out else ""
        ink = "primary" if stands_out else "secondary"
        parts.append(f'<text class="{ink}" x="{LEFT - 10}" y="{y + 4:.1f}" font-size="12" '
                     f'text-anchor="end"{weight}>{escape(label)}</text>')
        parts.append(f'<line class="{tone}-line" x1="{xl:.1f}" y1="{y:.1f}" x2="{xh:.1f}" '
                     f'y2="{y:.1f}" stroke-width="2" stroke-linecap="round"/>')
        parts.append(f'<circle class="{tone} ring" cx="{xl:.1f}" cy="{y:.1f}" r="5" '
                     f'stroke-width="2"><title>{escape(label)}, {escape(labels[0])}: '
                     f'{number(low, digits)} {escape(unit)}</title></circle>')
        parts.append(f'<circle class="surface {tone}-line" cx="{xh:.1f}" cy="{y:.1f}" r="4.5" '
                     f'stroke-width="2"><title>{escape(label)}, {escape(labels[1])}: '
                     f'{number(high, digits)} {escape(unit)}</title></circle>')
        # Each value on its dot's outer side.
        parts.append(f'<text class="{ink} tick" x="{xl - 10:.1f}" y="{y + 4:.1f}" font-size="12" '
                     f'text-anchor="end"{weight}>{number(low, digits)}</text>')
        parts.append(f'<text class="{ink} tick" x="{xh + 10:.1f}" y="{y + 4:.1f}" font-size="12"'
                     f'{weight}>{number(high, digits)}</text>')
    parts.append("</svg>")
    return "\n".join(parts)


PANEL_GAP = 28
PANEL_HEIGHT = 166
PLOT_TOP = 44
PLOT_HEIGHT = 90
PLOT_LEFT = 38
PLOT_RIGHT = 46
SERIES = ("accent", "second")


def trends(title, subtitle, columns, panels, legend=None):
    """Small multiples of measures over releases, a panel each, two to a
    row: `columns`, `(label, is_build)` of each release, in their order;
    `panels`, each `{"title", "unit", "digits", "budget", "series"}`,
    `series` one or two lists of a value for each release, or None where
    the release has none. Each panel has one scale from 0, its budget, if
    any, a dashed line, each release's value a dot, a build between
    releases a ring, and the last value written beside its dot; with a
    second series, `legend` names the two."""
    base = top_of(subtitle)
    rows = math.ceil(len(panels) / 2)
    top = base + 18
    height = top + rows * PANEL_HEIGHT + 6
    width = (WIDTH - 32 - PANEL_GAP) / 2
    parts = header(title, subtitle, height)
    # The legend: the series' colors where there are two, and the ring of a
    # build between releases.
    x = 16
    for index, name in enumerate(legend or []):
        parts.append(f'<circle class="{SERIES[index]}" cx="{x + 5}" cy="{base + 2}" r="4"/>')
        parts.append(f'<text class="secondary" x="{x + 14}" y="{base + 6}" font-size="12">{escape(name)}</text>')
        x += 32 + 7 * len(name)
    parts.append(f'<circle class="surface context-line" cx="{x + 5}" cy="{base + 2}" r="4" stroke-width="2"/>')
    parts.append(f'<text class="secondary" x="{x + 14}" y="{base + 6}" font-size="12">'
                 'a build between releases</text>')
    for index, panel in enumerate(panels):
        left = 16 + (index % 2) * (width + PANEL_GAP)
        y0 = top + (index // 2) * PANEL_HEIGHT
        parts.extend(trend_panel(panel, columns, left, y0, width))
    parts.append("</svg>")
    return "\n".join(parts)


def trend_panel(panel, columns, left, y0, width):
    """The parts of a panel of `trends`, at `left` and `y0`, `width` wide."""
    parts = [f'<text class="primary" x="{left:.1f}" y="{y0 + 14}" font-size="12" '
             f'font-weight="600">{escape(panel["title"])}</text>']
    digits, unit, budget = panel.get("digits", 1), panel.get("unit", ""), panel.get("budget")
    references = [r for r in panel.get("references", []) if r]
    values = [v for series in panel["series"] for v in series if v is not None]
    ticks = nice_ticks(max(values + ([budget] if budget else []) + [r["value"] for r in references]), 3)
    plot_left, plot_right = left + PLOT_LEFT, left + width - PLOT_RIGHT
    plot_top, plot_bottom = y0 + PLOT_TOP, y0 + PLOT_TOP + PLOT_HEIGHT
    ys = lambda value: plot_bottom - value / ticks[-1] * PLOT_HEIGHT
    step = (plot_right - plot_left) / max(len(columns) - 1, 1)
    xs = lambda index: plot_left + index * step
    for tick in ticks:
        y = ys(tick)
        parts.append(f'<line class="grid" x1="{plot_left:.1f}" y1="{y:.1f}" x2="{plot_right:.1f}" '
                     f'y2="{y:.1f}" stroke-width="1"/>')
        parts.append(f'<text class="muted tick" x="{plot_left - 6:.1f}" y="{y + 3.5:.1f}" font-size="10" '
                     f'text-anchor="end">{compact(tick)}</text>')
    for index, (label, build) in enumerate(columns):
        style = ' font-style="italic"' if build else ""
        parts.append(f'<text class="muted" x="{xs(index):.1f}" y="{plot_bottom + 15:.1f}" font-size="10" '
                     f'text-anchor="middle"{style}>{escape(label)}</text>')
    if budget is not None:
        y = ys(budget)
        label = panel.get("budget_label") or f"{budget:g}"
        parts.append(f'<line class="budget" x1="{plot_left:.1f}" y1="{y:.1f}" x2="{plot_right:.1f}" '
                     f'y2="{y:.1f}" stroke-width="1.5" stroke-dasharray="4 3">'
                     f'<title>budget: {escape(label)} {escape(unit)}</title></line>')
        # Its label on the side of the line away from the values.
        above = not values or sum(values) / len(values) <= budget
        ly = y - 5 if above else y + 12
        parts.append(f'<text class="secondary" x="{plot_right:.1f}" y="{ly:.1f}" font-size="10" '
                     f'text-anchor="end">budget {escape(label)}</text>')
    # The best of the other terminals, a dotted line of each series' tone,
    # and named under the panel's title, after a sample of the line.
    shown = [(which, r) for which, r in enumerate(panel.get("references", [])) if r]
    for which, reference in shown:
        y = ys(reference["value"])
        tone = "context" if len(panel["series"]) == 1 else SERIES[which]
        parts.append(f'<line class="{tone}-line" x1="{plot_left:.1f}" y1="{y:.1f}" x2="{plot_right:.1f}" '
                     f'y2="{y:.1f}" stroke-width="1.5" stroke-dasharray="1 3" stroke-linecap="round">'
                     f'<title>{escape(reference["label"])} {escape(unit)}</title></line>')
    if shown:
        label = shown[0][1]["label"] if len(shown) == 1 else panel.get("reference_label", shown[0][1]["label"])
        parts.append(f'<line class="context-line" x1="{left:.1f}" y1="{y0 + 26:.1f}" x2="{left + 15:.1f}" '
                     f'y2="{y0 + 26:.1f}" stroke-width="1.5" stroke-dasharray="1 3" stroke-linecap="round"/>')
        parts.append(f'<text class="muted" x="{left + 21:.1f}" y="{y0 + 29.5}" font-size="10">'
                     f'{escape(label)}</text>')
    ends = []
    for which, series in enumerate(panel["series"]):
        tone = SERIES[which]
        points = [(xs(i), ys(v), v, columns[i]) for i, v in enumerate(series) if v is not None]
        # The line, through the releases that have a value, one after another.
        for (x1, y1, _, _), (x2, y2, _, _) in zip(points, points[1:]):
            parts.append(f'<line class="{tone}-line" x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" '
                         f'y2="{y2:.1f}" stroke-width="2" stroke-linecap="round"/>')
        name = panel.get("names", [None, None])[which]
        for x, y, value, (label, build) in points:
            tip = f'{escape(label)}{", " + escape(name) if name else ""}: {number(value, digits)} {escape(unit)}'
            # A release's dot of its series' color in a ring of the surface; a
            # build's a ring of the color.
            style = f"surface {tone}-line" if build else f"{tone} ring"
            parts.append(f'<circle class="{style}" cx="{x:.1f}" cy="{y:.1f}" r="4" '
                         f'stroke-width="2"><title>{tip}</title></circle>')
        if points:
            ends.append(points[-1])
    # The last value of each series beside its dot, but where two would meet,
    # the first series' alone, the legend and the tooltips telling the rest.
    if len(ends) == 2 and abs(ends[0][1] - ends[1][1]) < 13:
        ends = ends[:1]
    for x, y, value, _ in ends:
        parts.append(f'<text class="primary tick" x="{x + 9:.1f}" y="{y + 4:.1f}" font-size="11" '
                     f'font-weight="600">{number(value, digits)}</text>')
    return parts
