"""Charts of the benches' reports, as SVG files that a Markdown page shows:
bars of one value each, of which one stands out, and pairs of a value
before and after a change. Each chart follows the reader's light or dark
appearance, and tells each mark's value as the pointer rests on it, where
the browser shows the SVG itself.

The colors: the accent blue for what the chart is about, a gray for the
rest, which recedes; text, grid and axes in their own grays.
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


def header(title, subtitle, height):
    return [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" '
        f'viewBox="0 0 {WIDTH} {height}" role="img" aria-label="{escape(title)}">',
        STYLE,
        f'<rect class="surface" width="{WIDTH}" height="{height}" rx="8"/>',
        f'<text class="primary" x="16" y="26" font-size="15" font-weight="600">{escape(title)}</text>',
        f'<text class="secondary" x="16" y="45" font-size="12">{escape(subtitle)}</text>',
    ]


def axis(parts, ticks, scale, bottom):
    for tick in ticks:
        x = LEFT + scale(tick)
        parts.append(f'<line class="grid" x1="{x:.1f}" y1="{TOP - 6}" x2="{x:.1f}" '
                     f'y2="{bottom}" stroke-width="1"/>')
        parts.append(f'<text class="muted tick" x="{x:.1f}" y="{bottom + 15}" font-size="11" '
                     f'text-anchor="middle">{compact(tick)}</text>')
    parts.append(f'<line class="base" x1="{LEFT}" y1="{TOP - 6}" x2="{LEFT}" y2="{bottom}" '
                 'stroke-width="1"/>')


def bar_path(x, y, length):
    """A bar from the baseline at `x`, rounded at its data end only."""
    r = min(4, length / 2, BAR / 2)
    return (f"M{x:.1f},{y:.1f} h{max(length - r, 0):.1f} "
            f"a{r},{r} 0 0 1 {r},{r} v{BAR - 2 * r:.1f} a{r},{r} 0 0 1 {-r},{r} "
            f"h{-max(length - r, 0):.1f} z")


def bars(title, subtitle, rows, unit, digits=0, order="ascending"):
    """Bars of `rows`, `(label, value, stands_out)`, each labelled with its
    value in `unit`, sorted as `order` says, the best first."""
    rows = sorted(rows, key=lambda row: row[1], reverse=order == "descending")
    height = TOP + BAND * len(rows) + 34
    bottom = TOP + BAND * len(rows) - 6
    ticks = nice_ticks(max(row[1] for row in rows))
    span = WIDTH - LEFT - RIGHT
    scale = lambda value: value / ticks[-1] * span
    parts = header(title, subtitle, height)
    axis(parts, ticks, scale, bottom)
    for index, (label, value, stands_out) in enumerate(rows):
        y = TOP + index * BAND + (BAND - BAR) / 2 - 6
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
    height = TOP + 22 + BAND * len(rows) + 34
    top = TOP + 22
    bottom = top + BAND * len(rows) - 6
    highest = max(max(row[1], row[2]) for row in rows)
    if budgets:
        highest = max([highest] + [b for b in budgets if b is not None])
    ticks = nice_ticks(highest)
    span = WIDTH - LEFT - RIGHT
    scale = lambda value: value / ticks[-1] * span
    parts = header(title, subtitle, height)
    # The legend, a dot of each beside its name.
    parts.append(f'<circle class="context" cx="22" cy="{TOP + 2}" r="5"/>')
    parts.append(f'<text class="secondary" x="32" y="{TOP + 6}" font-size="12">{escape(labels[0])}</text>')
    offset = 44 + 7 * len(labels[0])
    parts.append(f'<circle class="accent" cx="{offset}" cy="{TOP + 2}" r="5"/>')
    parts.append(f'<text class="secondary" x="{offset + 10}" y="{TOP + 6}" font-size="12">'
                 f'{escape(labels[1])}</text>')
    if budgets:
        offset += 22 + 7 * len(labels[1])
        parts.append(f'<line class="budget" x1="{offset}" y1="{TOP - 5}" x2="{offset}" '
                     f'y2="{TOP + 9}" stroke-width="2"/>')
        parts.append(f'<text class="secondary" x="{offset + 8}" y="{TOP + 6}" font-size="12">'
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
    height = 300
    top, bottom = TOP + 34, height - 44
    left, right = 52, 24
    ticks = nice_ticks(max(max(before), max(after)), 4)
    scale = lambda value: value / ticks[-1] * (bottom - top)
    parts = header(title, subtitle, height)
    parts.append(f'<rect class="context" x="16" y="{TOP - 4}" width="12" height="12" rx="2"/>')
    parts.append(f'<text class="secondary" x="34" y="{TOP + 6}" font-size="12">{escape(labels[0])}</text>')
    offset = 50 + 7 * len(labels[0])
    parts.append(f'<rect class="accent" x="{offset}" y="{TOP - 4}" width="12" height="12" rx="2"/>')
    parts.append(f'<text class="secondary" x="{offset + 18}" y="{TOP + 6}" font-size="12">'
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
