"""Generates assets/readme/social-preview.svg (640x320, GitHub's social preview size) from the
hero's drawing code. Render to PNG: see the end of this file's docstring in RELEASE.md workflow,
or headless Chrome --window-size=640,320 --screenshot."""
import importlib.util, pathlib, sys
here = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("hero", here / "make_hero.py")
src = (here / "make_hero.py").read_text(encoding="utf-8").split("TX, TY, TW, TH")[0]
hero = {}
exec(src, hero)
INK, CARD, TEXT2, MUTED, MONO = hero["INK"], hero["CARD"], hero["TEXT2"], hero["MUTED"], hero["MONO"]
FILLS, SCAN, LABEL = hero["FILLS"], hero["SCAN"], hero["LABEL"]

tiles = hero["squarify"]([(v, (n, v, i)) for i, (n, v) in enumerate(SCAN)], 36, 190, 568, 94)
parts = []
for x, y, w, h, (name, gib, i) in tiles:
    fill = FILLS[i]
    parts.append(f'  <rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="4" fill="{fill}" stroke="{INK}" stroke-width="2.5"/>')
    if hero["width_of"](name, 15) <= w - 16 and h > 26:
        parts.append(f'  <text x="{x + 8:.1f}" y="{y + 21:.1f}" font-size="15" font-weight="700" fill="{LABEL[fill]}">{name}</text>')

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="640" height="320" viewBox="0 0 640 320" role="img" aria-labelledby="t d">
  <title id="t">TimberStat</title>
  <desc id="d">TimberStat: see where your disk space went, then clean it up. A treemap of a real C: drive scan.</desc>
  <rect width="640" height="320" fill="{CARD}"/>
  <rect x="6" y="6" width="628" height="308" rx="18" fill="none" stroke="{INK}" stroke-width="4"/>
  <g font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif">
{hero["logo"](30, 22, 86)}
  <text x="126" y="84" font-size="50" font-weight="800" fill="{INK}" letter-spacing="-1">TimberStat</text>
  <text x="38" y="138" font-size="21" fill="{TEXT2}">See where your disk space went, then clean it up.</text>
  <text x="38" y="170" font-size="14" fill="{MUTED}" font-family="{MONO}">Disk analyser + file manager · Windows · Linux · Rust</text>
{chr(10).join(parts)}
  </g>
</svg>
"""
(here.parent / "social-preview.svg").write_text(svg, encoding="utf-8")
print("ok")
