#!/usr/bin/env python3
"""Redraw apps/manager/public/logo.svg from the vendored Geist Mono ExtraBold.

The mark is the brand's: a near-black rounded square carrying a lowercase `m`.
The glyph is emitted as a path rather than as text so the favicon, the installer
icon and the site all render it identically with no font available.

Needs fonttools + brotli (a woff2 is brotli-compressed):
    uv venv .fontenv && uv pip install --python .fontenv/bin/python fonttools brotli
"""
import os

from fontTools.misc.transform import Identity
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT = os.path.join(ROOT, "packages/shared/src/fonts/geistmono-800-latin.woff2")
OUT = os.path.join(ROOT, "apps/manager/public/logo.svg")

BOX = 40.0        # the viewBox the app, the installer and the favicon all use
RADIUS = 9.0      # the squircle of the brand's own favicon
GLYPH_H = 16.0    # x-height of the m inside that box

font = TTFont(FONT)
glyphs = font.getGlyphSet()
glyph = glyphs[font.getBestCmap()[ord("m")]]

bounds = BoundsPen(glyphs)
glyph.draw(bounds)
x0, y0, x1, y1 = bounds.bounds

scale = GLYPH_H / (y1 - y0)
tx = (BOX - (x1 - x0) * scale) / 2 - x0 * scale
ty = (BOX + (y1 - y0) * scale) / 2 + y0 * scale

# two decimals is plenty at 40px, and keeps the file readable
pen = SVGPathPen(glyphs, ntos=lambda v: ('%.2f' % v).rstrip('0').rstrip('.'))
glyph.draw(TransformPen(pen, Identity.translate(tx, ty).scale(scale, -scale)))

with open(OUT, "w") as fh:
    fh.write(
        '<svg width="40" height="40" viewBox="0 0 40 40" fill="none"'
        ' xmlns="http://www.w3.org/2000/svg" role="img" aria-label="MXB App">\n'
        "  <!-- The mxbsecure mark: a near-black rounded square carrying the brand's\n"
        "       lowercase m, set in Geist Mono ExtraBold. The glyph is taken from the\n"
        "       vendored woff2 and written out as a path, so it renders identically\n"
        "       wherever it is used with no font present. Regenerate with\n"
        "       scripts/make-logo.py. -->\n"
        '  <rect width="40" height="40" rx="9" fill="#0b0b0c"/>\n'
        '  <path d="%s" fill="#ffffff"/>\n</svg>\n' % pen.getCommands()
    )
print("wrote", os.path.relpath(OUT, ROOT))
