#!/usr/bin/env python3
"""Build the surface brand kit in docs/assets/ from the definitions in this file.

The mark is an S: five bars — three horizontal, two risers — unioned into one
shape, with its two outer tips cut back at 45 degrees. The bars are the language
the dashboard already draws in, and 45 degrees is the angle of the hexagon the
mark used to be, so the geometry survives the hexagon's removal. Every asset is a
cut of that one shape, so it lives here once and the SVGs are generated rather
than hand-edited.

The wordmark is "surface" in Google Sans Flex 500, converted to outlines. That
conversion is the only reason this script exists: an SVG loaded through an
`<img>` tag — which is how both the README and the docs hero load the lockup —
cannot fetch a webfont, so a live `<text>` element would silently fall back to
whatever the viewer happens to have installed. Drawn glyphs render the same
everywhere.

    pip install fonttools          # outlines the wordmark
    brew install librsvg           # provides rsvg-convert, for the PNG cuts
    python scripts/brand.py

The font is fetched from Google Fonts and cached under .cache/; point
SURFACE_BRAND_FONT at a local Google Sans Flex 500 TTF to skip the download.
"""

from __future__ import annotations

import math
import os
import pathlib
import subprocess
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASSETS = ROOT / "docs" / "assets"
CACHE = ROOT / ".cache"

# Google Sans Flex, weight 500, as served by Google Fonts. The hashed filename
# is a content address: when Google ships a new version this 404s rather than
# quietly changing the letterforms, which is the failure mode we want.
FONT_URL = (
    "https://fonts.gstatic.com/s/googlesansflex/v21/"
    "t5sJIQcYNIWbFgDgAAzZ34auoVyXkJCOvp3SFWJbN5hF8Ju1x6sKCyp0l9sI40swNJwInycYAJzz"
    "0m7kJ4qFQOJBOjLvDSndo0SKMpKSTzwliVdHAy4bxTDHg_ugnAakp_mbycs.ttf"
)

WORDMARK_TEXT = "surface"
WORDMARK_SIZE = 168.0  # px on the lockup's 256-tall grid
WORDMARK_TRACKING = -2.0  # px between advances; display sizes want it tight
WORDMARK_BASELINE = 188.0  # puts the glyph bounding box on the mark's own axis
WORDMARK_START_X = 288.0  # leaves a gap of ~25% of the mark's width

# --------------------------------------------------------------------------- #
# Palette — the two ends of the neutral ramp in docs/stylesheets/extra.css.
# --------------------------------------------------------------------------- #
DARK = "#181818"  # --site-c-ink
LIGHT = "#f5f5f0"  # --site-c-paper
FADE_FROM = "#8c8c8a"  # --site-c-smoke

# --------------------------------------------------------------------------- #
# Geometry. The mark is one shape, built from bars on a 1024 grid; `s_mark()`
# writes it as an absolute SVG path. It used to be a flat-top hexagon with a cell
# in it — the 45-degree chamfers on the S are what is left of that.
# --------------------------------------------------------------------------- #
def s_lines(cx: float, cy: float, r: float, lines: int, weight: float,
            seam: float, amp: float, taper: float = 0.62) -> str:
    """The mark: an S carved out of horizontal scan lines inside a disc.

    Rows of tapered bars fill a circle; a sine channel of width `seam` swings
    `amp` either side of centre and cuts them, and the gap it leaves is the S.
    The bars are the language the dashboard already draws in, and the sine is the
    same one the glyph field's waves ride on — the mark is the page's two motifs
    in one shape.

    Each bar is thick where it meets the S and tapers to a point at the rim, so
    the letter has a hard edge and the disc a feathered one. That asymmetry is
    what stops the disc reading as a circle with a squiggle in it.

    One full sine period over the height, phase pi: without the phase flip the
    channel swings right-then-left and draws a mirrored S.
    """
    out = []
    span = 2 * r
    for i in range(lines):
        # Half a step of inset, so the first and last rows sit inside the disc
        # rather than on its tangent where they would render as specks.
        y = cy - r + span * (i + 0.5) / lines
        dy = y - cy
        if abs(dy) >= r:
            continue
        half = math.sqrt(max(0.0, r * r - dy * dy))
        t = (y - (cy - r)) / span
        centre = cx + amp * math.sin(2 * math.pi * t + math.pi)
        # Thinner towards the poles, so the disc's edge stays even.
        w = weight * (0.55 + 0.45 * math.sqrt(max(0.0, 1 - (dy / r) ** 2)))

        for side in (-1, 1):
            inner = centre + side * seam / 2
            outer = cx + side * half
            if (outer - inner) * side <= 12:
                continue
            mid = inner + (outer - inner) * (1 - taper)
            out.append(f"M {inner:.1f} {y - w / 2:.1f} L {mid:.1f} {y - w / 2:.1f} "
                       f"L {outer:.1f} {y:.1f} L {mid:.1f} {y + w / 2:.1f} "
                       f"L {inner:.1f} {y + w / 2:.1f} Z")
    return " ".join(out)


def s_solid(cx: float, cy: float, r: float, seam: float, amp: float,
            steps: int = 72) -> str:
    """The small cut: a solid disc with the same S channel knocked out of it.

    Below about 24px the scan lines stop being lines — they land inside a device
    pixel and antialias into a grey disc. This is the same drawing with the
    striping dropped, which is what keeps the S crisp in a browser tab.

    Filled `evenodd`, and the channel is clipped to the disc row by row: a
    subpath that escaped the disc would flip the winding and fill *outside* it.
    """
    ring = (f"M {cx - r:.1f} {cy:.1f} A {r:.1f} {r:.1f} 0 0 1 {cx + r:.1f} {cy:.1f} "
            f"A {r:.1f} {r:.1f} 0 0 1 {cx - r:.1f} {cy:.1f} Z")
    left, right = [], []
    for i in range(steps + 1):
        t = i / steps
        y = cy - r + 2 * r * t
        dy = y - cy
        half = math.sqrt(max(0.0, r * r - dy * dy))
        c = cx + amp * math.sin(2 * math.pi * t + math.pi)
        lo, hi = max(c - seam / 2, cx - half), min(c + seam / 2, cx + half)
        if hi <= lo:
            continue
        left.append((lo, y))
        right.append((hi, y))
    if len(left) < 2:
        return ring
    pts = left + right[::-1]
    body = " ".join(f"{x:.1f} {y:.1f}" for x, y in pts)
    return f"{ring} M {body} Z"


# --------------------------------------------------------------------------- #
# The cuts. Each size is its own drawing rather than a scaled copy: line art
# scaled down thins out, and the numbers below are what each size can carry.
# --------------------------------------------------------------------------- #
MARK_S = s_lines(512, 512, 430, lines=35, weight=17, seam=92, amp=190)

# The lockup carries the mark on an 880x256 grid, beside the wordmark. Fewer
# lines than the large cut: at this size 35 of them close up.
LOCKUP_S = s_lines(128, 128, 107, lines=23, weight=6.2, seam=26, amp=48)

# The favicon: no striping at all, and pushed wider so the disc fills the 16px
# grid a browser tab draws on.
FAVICON_S = s_solid(512, 512, 452, seam=140, amp=200)

MARK_NOTE = """  <!-- surface mark: an S carved out of scan lines. Tapered bars fill a disc; a
       sine channel cuts them, and the gap is the letter. The bars are the
       dashboard's own language and the sine is the one the docs' glyph field
       rides on. Generated — edit scripts/brand.py, not this file. -->"""

LOCKUP_NOTE = """  <!-- The mark beside "surface" in Google Sans Flex 500, converted to outlines
       rather than left as <text>: both the README and the docs hero load this
       through an <img>, and an image cannot fetch a webfont. Fewer scan lines
       than the large cut, because at this size the fine ones close up. -->"""

FAVICON_NOTE = """  <!-- The favicon cut: the same disc and the same S channel, with the striping
       dropped. Below ~24px the lines land inside a device pixel and antialias
       into a grey disc, so the small cut is two shapes instead of forty. -->"""


# --------------------------------------------------------------------------- #
# Wordmark outlining
# --------------------------------------------------------------------------- #
def font_path() -> pathlib.Path:
    """Google Sans Flex 500, from SURFACE_BRAND_FONT or the Google Fonts CDN."""
    override = os.environ.get("SURFACE_BRAND_FONT")
    if override:
        return pathlib.Path(override).expanduser()

    cached = CACHE / "google-sans-flex-500.ttf"
    if not cached.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        print(f"fetching {FONT_URL.rsplit('/', 1)[-1][:24]}… -> {cached}")
        request = urllib.request.Request(FONT_URL, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(request) as response:
            cached.write_bytes(response.read())
    return cached


def wordmark() -> tuple[str, tuple[float, float, float, float]]:
    """Outline WORDMARK_TEXT. Returns the path data and its bounding box."""
    from fontTools.misc.transform import Transform
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.ttLib import TTFont

    font = TTFont(font_path())
    scale = WORDMARK_SIZE / font["head"].unitsPerEm
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()

    parts: list[str] = []
    bounds: list[float] | None = None
    pen_x = WORDMARK_START_X
    for char in WORDMARK_TEXT:
        glyph = glyphs[cmap[ord(char)]]
        # Font units go up, SVG goes down, hence the negative y scale.
        transform = Transform(scale, 0, 0, -scale, pen_x, WORDMARK_BASELINE)

        svg_pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.1f}")
        glyph.draw(TransformPen(svg_pen, transform))
        if commands := svg_pen.getCommands():
            parts.append(commands)

        bounds_pen = BoundsPen(glyphs)
        glyph.draw(TransformPen(bounds_pen, transform))
        if bounds_pen.bounds:
            x0, y0, x1, y1 = bounds_pen.bounds
            if bounds is None:
                bounds = [x0, y0, x1, y1]
            else:
                bounds = [
                    min(bounds[0], x0),
                    min(bounds[1], y0),
                    max(bounds[2], x1),
                    max(bounds[3], y1),
                ]

        pen_x += glyph.width * scale + WORDMARK_TRACKING

    if bounds is None:  # pragma: no cover - WORDMARK_TEXT is never blank
        raise SystemExit("wordmark produced no outlines")
    return " ".join(parts), tuple(bounds)


# --------------------------------------------------------------------------- #
# SVG templates
# --------------------------------------------------------------------------- #
def svg_mark(paint: str, defs: str = "") -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024" role="img" aria-label="surface logo">
  <title>surface</title>
{MARK_NOTE}
{defs}  <path fill="{paint}" d="{MARK_S}"/>
</svg>
"""


def svg_lockup(paint: str, glyphs: str) -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 880 256" width="880" height="256" role="img" aria-label="surface">
  <title>surface</title>
{LOCKUP_NOTE}
  <path fill="{paint}" d="{LOCKUP_S}"/>
  <path fill="{paint}" d="{glyphs}"/>
</svg>
"""


def svg_favicon() -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024" role="img" aria-label="surface">
  <title>surface</title>
{FAVICON_NOTE}
  <style>
    .mark {{ fill: {DARK}; }}
    @media (prefers-color-scheme: dark) {{ .mark {{ fill: {LIGHT}; }} }}
  </style>
  <path class="mark" fill-rule="evenodd" d="{FAVICON_S}"/>
</svg>
"""


# The gradient spans the ring's own bounding box in user space, so the ring and
# the cell are cut from one sweep instead of each getting its own.
FADE_DEFS = f"""  <defs>
    <linearGradient id="fade" gradientUnits="userSpaceOnUse" x1="92" y1="148" x2="932" y2="876">
      <stop offset="0" stop-color="{FADE_FROM}"/><stop offset="1" stop-color="{DARK}"/>
    </linearGradient>
  </defs>
"""


# --------------------------------------------------------------------------- #
# PNG cuts
# --------------------------------------------------------------------------- #
# (source svg, output png, width, height). The lockup PNGs are what the README
# loads, since GitHub strips the CSS an SVG would need to follow the theme.
RASTERS = [
    ("logo-mark.svg", "logo-512.png", 512, 512),
    ("logo-mark-inverse.svg", "logo-512-inverse.png", 512, 512),
    ("logo-lockup.svg", "logo-lockup.png", 1760, 512),
    ("logo-lockup-inverse.svg", "logo-lockup-inverse.png", 1760, 512),
    ("logo-mark.svg", "apple-touch-icon.png", 180, 180),
    ("favicon.svg", "favicon-32.png", 32, 32),
    ("favicon.svg", "favicon-16.png", 16, 16),
]


def rasterise() -> None:
    for source, target, width, height in RASTERS:
        subprocess.run(
            [
                "rsvg-convert",
                "--width", str(width),
                "--height", str(height),
                "--output", str(ASSETS / target),
                str(ASSETS / source),
            ],
            check=True,
        )
        print(f"wrote {target} ({width}x{height})")


def main() -> int:
    glyphs, bounds = wordmark()
    print(
        "wordmark bounds "
        f"x {bounds[0]:.1f}..{bounds[2]:.1f}  y {bounds[1]:.1f}..{bounds[3]:.1f}"
    )

    for name, body in {
        "logo-mark.svg": svg_mark(DARK),
        "logo-mark-inverse.svg": svg_mark(LIGHT),
        "logo-mark-fade.svg": svg_mark("url(#fade)", defs=FADE_DEFS),
        "logo-lockup.svg": svg_lockup(DARK, glyphs),
        "logo-lockup-inverse.svg": svg_lockup(LIGHT, glyphs),
        "favicon.svg": svg_favicon(),
    }.items():
        (ASSETS / name).write_text(body)
        print(f"wrote {name} ({len(body)} bytes)")

    rasterise()
    return 0


if __name__ == "__main__":
    sys.exit(main())
