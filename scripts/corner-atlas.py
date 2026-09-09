#!/usr/bin/env python3
"""Draw the corners `trackstats::corner_atlas` dumped, so their shape can be looked at.

    FROST_TRACK=…/indiana.pkz FROST_OUT=/tmp/atlas/indiana \
      cargo test --bin mxb-app -- --ignored --nocapture corner_atlas
    python3 scripts/corner-atlas.py /tmp/atlas/indiana

Five panels a corner, because "what shape are the bumps" is five different questions:

  shape    the turn itself, hillshaded — how it bends, and which way it leans
  bumps    the same ground with a 6 m running mean taken out, which is the only way the
           ruts show at all: they are centimetres on a hillside of metres
  ground   what the game actually paints there, the layer stack composited through its masks
  across   three cuts through the line — entry, apex, exit — where a rut is a shape and not
           a statistic
  along    the line's own profile, detrended, which is where braking bumps live

Needs numpy, matplotlib and pillow."""
import json
import struct
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

# Metres either way on the bumps panel. Fixed, so every picture is on the same scale.
BUMP_SCALE_M = 0.25

INK = "#f2f3f5"
GROUND = "#0c0c0e"
PANEL = "#141417"
LINE = "#9ccfec"


def load_patch(path):
    b = path.read_bytes()
    assert b[:4] == b"FRCA", b[:4]
    pw, ph = struct.unpack_from("<II", b, 4)
    x0, z0, mps = struct.unpack_from("<fff", b, 12)
    o = 24
    n = pw * ph
    H = np.frombuffer(b, "<f4", n, o).reshape(ph, pw).copy()
    o += 4 * n
    RGB = np.frombuffer(b, "u1", n * 3, o).reshape(ph, pw, 3).copy()
    o += 3 * n
    (sc,) = struct.unpack_from("<I", b, o)
    o += 4
    S = np.frombuffer(b, "<f4", sc * 2, o).reshape(sc, 2).copy()
    return dict(w=pw, h=ph, x0=x0, z0=z0, mps=mps, H=H, RGB=RGB, S=S)


def detrend2d(H, half):
    """Subtract a running mean, so centimetre bumps survive a hillside of metres."""
    k = 2 * half + 1
    pad = np.pad(H, half, mode="edge")
    csum = np.cumsum(np.cumsum(pad, axis=0), axis=1)
    csum = np.pad(csum, ((1, 0), (1, 0)))
    box = (
        csum[k:, k:] - csum[:-k, k:] - csum[k:, :-k] + csum[:-k, :-k]
    ) / (k * k)
    return H - box[: H.shape[0], : H.shape[1]]


def hillshade(H, mps, az=315.0, alt=45.0):
    gz, gx = np.gradient(H, mps)
    slope = np.arctan(np.hypot(gx, gz))
    aspect = np.arctan2(-gz, gx)
    az, alt = np.radians(az), np.radians(alt)
    v = np.sin(alt) * np.cos(slope) + np.cos(alt) * np.sin(slope) * np.cos(az - aspect)
    return np.clip(v, 0, 1)


def cross_sections(P, frac):
    """A cut across the line at `frac` along it: (offset metres, height metres)."""
    S, mps = P["S"], P["mps"]
    i = int(np.clip(frac * (len(S) - 1), 1, len(S) - 2))
    p, nxt = S[i], S[i + 1]
    d = nxt - p
    n = np.array([-d[1], d[0]])
    n = n / (np.hypot(*n) or 1.0)
    ts = np.arange(-9.0, 9.0 + 1e-6, mps / 2)
    xs, zs = p[0] + n[0] * ts, p[1] + n[1] * ts
    ix = np.clip(((xs - P["x0"]) / mps).astype(int), 0, P["w"] - 1)
    iz = np.clip(((zs - P["z0"]) / mps).astype(int), 0, P["h"] - 1)
    return ts, P["H"][iz, ix]


def along_profile(P):
    S, mps = P["S"], P["mps"]
    ix = np.clip(((S[:, 0] - P["x0"]) / mps).astype(int), 0, P["w"] - 1)
    iz = np.clip(((S[:, 1] - P["z0"]) / mps).astype(int), 0, P["h"] - 1)
    h = P["H"][iz, ix]
    d = np.concatenate([[0.0], np.cumsum(np.hypot(np.diff(S[:, 0]), np.diff(S[:, 1])))])
    return d, h


def running(v, half):
    k = 2 * half + 1
    return np.convolve(np.pad(v, half, mode="edge"), np.ones(k) / k, mode="valid")


def draw(out_dir, meta, c):
    P = load_patch(out_dir / c["patch"])
    H = np.where(np.isfinite(P["H"]), P["H"], np.nanmedian(P["H"]))
    bumps = detrend2d(H, max(1, int(round(3.0 / P["mps"]))))
    extent = [0, P["w"] * P["mps"], 0, P["h"] * P["mps"]]
    sx = (P["S"][:, 0] - P["x0"])
    sz = (P["S"][:, 1] - P["z0"])

    fig = plt.figure(figsize=(17, 4.2), facecolor=GROUND)
    fig.subplots_adjust(left=0.03, right=0.99, top=0.86, bottom=0.14, wspace=0.22)
    grid = fig.add_gridspec(1, 5, width_ratios=[1, 1, 1, 1.05, 1.05])

    def sky(ax, title):
        ax.set_facecolor(PANEL)
        ax.set_title(title, color=INK, fontsize=10, pad=6)
        for s in ax.spines.values():
            s.set_color("#333")
        ax.tick_params(colors="#8a8a92", labelsize=7)

    ax = fig.add_subplot(grid[0])
    sky(ax, "shape")
    ax.imshow(hillshade(H, P["mps"]), cmap="gray", origin="lower", extent=extent, vmin=0, vmax=1)
    ax.plot(sx, sz, color=LINE, lw=1.4)
    ax.set_aspect("equal")

    ax = fig.add_subplot(grid[1])
    sky(ax, "bumps  (6 m detrend)")
    # One fixed scale for every picture, always. Normalising each panel to its own range
    # stretches a smooth corner to fill the same colours as a rough one, so two tracks drawn
    # side by side look equally chopped up whatever they actually measure — which is exactly
    # backwards for a comparison, and it made ours look redder than Indiana while being
    # measurably smoother.
    ax.imshow(bumps, cmap="RdBu_r", origin="lower", extent=extent, vmin=-BUMP_SCALE_M, vmax=BUMP_SCALE_M)
    ax.plot(sx, sz, color="#111", lw=0.9, alpha=0.7)
    ax.set_aspect("equal")

    ax = fig.add_subplot(grid[2])
    sky(ax, "ground  (as painted)")
    ax.imshow(P["RGB"], origin="lower", extent=extent)
    ax.plot(sx, sz, color=LINE, lw=1.0, alpha=0.85)
    ax.set_aspect("equal")

    ax = fig.add_subplot(grid[3])
    sky(ax, "across the line")
    for frac, colour, name in ((0.15, "#6fd99a", "entry"), (0.5, "#e8bd6a", "apex"), (0.85, "#ef8078", "exit")):
        t, h = cross_sections(P, frac)
        ax.plot(t, h - h.mean(), color=colour, lw=1.3, label=name)
    ax.axvline(0, color="#555", lw=0.8, ls=":")
    ax.set_xlabel("metres off the line", color="#8a8a92", fontsize=8)
    ax.set_ylabel("m", color="#8a8a92", fontsize=8)
    ax.legend(fontsize=7, facecolor=PANEL, edgecolor="#333", labelcolor=INK)

    ax = fig.add_subplot(grid[4])
    sky(ax, "along the line")
    d, h = along_profile(P)
    ax.plot(d, h - running(h, max(1, int(2.0 / max(P["mps"], 1e-3)))), color="#9ccfec", lw=1.0)
    ax.set_xlabel("metres round the turn", color="#8a8a92", fontsize=8)
    ax.set_ylabel("m", color="#8a8a92", fontsize=8)

    head = (
        f"{meta['track']} · turn {c['n']} at {c['atM']:.0f} m — "
        f"{c['turnDeg']:.0f}° {c['direction']} over {c['lengthM']:.0f} m, "
        f"{c['arcs']} arcs, r {c['radiusTightestM']:.0f}–{c['radiusWidestM']:.0f} m, rise {c['riseM']:.2f} m"
    )
    if c.get("grooves") is not None:
        head += (
            f"   |   {c['grooves']:.1f} grooves at {c['spacingM']:.2f} m, floor {c['floorM']:.2f} m, "
            f"wall {c['wallDeg']:.0f}°, chatter {c['chatterM']*100:.1f} cm"
        )
    fig.suptitle(head, color=INK, fontsize=11, y=0.97)
    png = out_dir / f"corner{c['n']}.png"
    fig.savefig(png, dpi=110, facecolor=GROUND)
    plt.close(fig)
    return png


def main():
    out_dir = Path(sys.argv[1])
    meta = json.loads((out_dir / "corners.json").read_text())
    pngs = [draw(out_dir, meta, c) for c in meta["corners"]]
    # One sheet for the track, so four turns can be compared at a glance.
    from PIL import Image

    ims = [Image.open(p) for p in pngs]
    w = max(i.width for i in ims)
    sheet = Image.new("RGB", (w, sum(i.height for i in ims)), (12, 12, 14))
    y = 0
    for i in ims:
        sheet.paste(i, (0, y))
        y += i.height
    dest = out_dir / f"{meta['track']}-corners.png"
    sheet.save(dest)
    print(dest)


if __name__ == "__main__":
    main()
