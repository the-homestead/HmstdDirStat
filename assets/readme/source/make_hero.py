"""Generates assets/readme/hero.svg: the TimberStat title next to a treemap of a real C: scan.

Drawn in the logo's style: thick dark-brown outlines on flat olive, amber, sage and cream.
Sizes are the top-level folders from the scan in assets/readme/screenshot.png (GiB).
Run from the repo root: python assets/readme/source/make_hero.py
"""

W, H = 1200, 380
INK, CARD, CREAM = "#351507", "#f7ecd4", "#fbe8c0"
TEXT2, MUTED = "#4a2a12", "#7a5a3a"
AMBER, OLIVE, SAGE, BARK, MOSS = "#f2a823", "#5d6327", "#9aa456", "#8a5a2b", "#7d8236"
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"

SCAN = [  # name, GiB (C:\, 7,176,706 files, 1.2 TiB)
    ("Users", 668.1), ("Program Files", 130.1), ("Program Files (x86)", 115.0),
    ("Riot Games", 82.4), ("ProgramData", 73.6), ("Dev", 71.0),
    ("Windows", 58.5), ("pagefile.sys", 31.2),
]
FILLS = [AMBER, OLIVE, SAGE, BARK, MOSS, SAGE, OLIVE, BARK]
LABEL = {AMBER: INK, SAGE: INK, OLIVE: CREAM, BARK: CREAM, MOSS: CREAM}


def squarify(items, x, y, w, h):
    """Squarified treemap (Bruls et al.). items: [(value, payload)], largest first."""
    out, items = [], list(items)
    while items:
        total = sum(v for v, _ in items)
        area = w * h
        short = min(w, h)
        row, best = [], float("inf")
        for item in items:
            trial = row + [item]
            side = sum(v for v, _ in trial) / total * area / short
            worst = max(max(a / side / side, side * side / a)
                        for a in (v / total * area for v, _ in trial))
            if worst > best:
                break
            row, best = trial, worst
        s = sum(v for v, _ in row) / total * area
        if w >= h:
            cw, cy = s / h, y
            for v, p in row:
                ch = v / total * area / cw
                out.append((x, cy, cw, ch, p)); cy += ch
            x, w = x + cw, w - cw
        else:
            rh, cx = s / w, x
            for v, p in row:
                rw = v / total * area / rh
                out.append((cx, y, rw, rh, p)); cx += rw
            y, h = y + rh, h - rh
        items = items[len(row):]
    return out


def logo(x, y, s):
    """The folder mark, redrawn: olive folder with a cream rim, three files shrinking in size."""
    k = s / 100
    return f"""    <g transform="translate({x} {y}) scale({k:.3f})" stroke="{INK}" stroke-width="5" stroke-linejoin="round">
      <path d="M 14 20 Q 14 12 22 12 L 42 12 Q 46 12 48 15 L 52 20 L 84 20 Q 90 20 89.5 26 L 87 56 L 14 56 Z" fill="{CREAM}"/>
      <path d="M 18 24 Q 18 17 24 17 L 41 17 L 47 24 L 83 24 Q 85.5 24 85 27 L 83 54 L 18 54 Z" fill="{OLIVE}" stroke="none"/>
      <path d="M 16 28 Q 16 23 21 23 L 35 23 Q 38 23 40 26 L 42 29 L 47 29 Q 51 29 51 33 L 51 56 L 16 56 Z" fill="{AMBER}"/>
      <path d="M 53 42 Q 53 39 56 39 L 61 39 L 64 43 L 67 43 Q 70 43 70 46 L 70 56 L 53 56 Z" fill="{SAGE}"/>
      <path d="M 72 49 Q 72 46 75 46 L 78 46 L 80 49 L 82 49 Q 85 49 85 52 L 85 56 L 72 56 Z" fill="{CREAM}"/>
      <path d="M 10 53 L 90 53 Q 94 53 93.5 57 L 88 85 Q 87 89 83 89 L 17 89 Q 13 89 12 85 L 6.5 57 Q 6 53 10 53 Z" fill="{OLIVE}"/>
      <path d="M 11 57.5 L 89 57.5" stroke="{CREAM}" stroke-width="3.5" stroke-linecap="round"/>
    </g>"""


def width_of(text, size):
    return len(text) * size * 0.55


def label_lines(name, size, room):
    """The name on one line, or split at a space onto two, or None when it does not fit."""
    if width_of(name, size) <= room:
        return [name]
    if " " in name:
        head, tail = name.split(" ", 1)
        if max(width_of(head, size), width_of(tail, size)) <= room:
            return [head, tail]
    return None


TX, TY, TW, TH = 640, 72, 504, 260
tiles = squarify([(v, (n, v, i)) for i, (n, v) in enumerate(SCAN)], TX, TY, TW, TH)
parts = []
for x, y, w, h, (name, gib, i) in tiles:
    fill = FILLS[i]
    parts.append(f'    <rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="6" fill="{fill}" stroke="{INK}" stroke-width="3.5"/>')
    size = 22 if w > 200 else 18
    lines = label_lines(name, size, w - 22)
    if not lines or h < 30 + 22 * (len(lines) - 1):
        continue
    for n, line in enumerate(lines):
        parts.append(f'    <text x="{x + 11:.1f}" y="{y + 27 + n * 21:.1f}" font-size="{size}" font-weight="700" fill="{LABEL[fill]}">{line}</text>')
    size_y = y + 27 + len(lines) * 21 + 2
    if size_y + 4 < y + h - 6 and width_of(f"{gib:.1f} GiB", 18) <= w - 22:
        parts.append(f'    <text x="{x + 11:.1f}" y="{size_y:.1f}" font-size="18" fill="{LABEL[fill]}" opacity="0.85" font-family="{MONO}">{gib:.1f} GiB</text>')

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" aria-labelledby="title desc">
  <title id="title">TimberStat</title>
  <desc id="desc">TimberStat, a disk usage analyser and file manager for Windows and Linux. Beside the name, a treemap of a real 1.2 TiB C: drive scan: Users 668 GiB, Program Files 130 GiB, Program Files (x86) 115 GiB and smaller folders.</desc>
  <rect x="3" y="3" width="{W - 6}" height="{H - 6}" rx="26" fill="{CARD}" stroke="{INK}" stroke-width="5"/>
  <g font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif">
{logo(50, 46, 120)}
    <text x="186" y="134" font-size="76" font-weight="800" fill="{INK}" letter-spacing="-1">TimberStat</text>
    <text x="60" y="222" font-size="32" fill="{TEXT2}">See where your disk space went,</text>
    <text x="60" y="262" font-size="32" fill="{TEXT2}">then clean it up.</text>
    <text x="60" y="322" font-size="20" fill="{MUTED}" font-family="{MONO}">Windows · Linux · Rust + GPUI · GPL-3.0</text>
    <text x="{TX}" y="54" font-size="20" fill="{MUTED}" font-family="{MONO}">C:\\ · 7,176,706 files · 1.2 TiB</text>
{chr(10).join(parts)}
  </g>
</svg>
"""
open("assets/readme/hero.svg", "w", encoding="utf-8").write(svg)
print("tiles", [(p[0], round(w), round(h)) for _, _, w, h, p in tiles])
