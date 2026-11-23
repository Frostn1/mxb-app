#!/usr/bin/env python3
"""What a published lap measures, read out of the track's own `.trh`.

`tracked -merge` writes the `.tcl` centreline the builder typed into the height file and it
stays there — length, signed radius and turn per segment — so a real track's layout reads as
numbers rather than as a picture. This is the instrument the walk in `track-walk.py` is
calibrated against, and it runs on the zips in `~/Projects/pkz` in a couple of seconds.

    python3 scripts/track-survey.py                       # the published corpus
    python3 scripts/track-survey.py path/to/track.zip     # one track

A **corner** here is same-handed turning under a 300 m radius with no more than ten metres of
run let into it. Both halves of that matter: Motorcycling Australia defines a curve as "a
direction change > 15 degrees with a radius under 300 m", and a published corner is three to
five arcs that tighten into the apex and release out, so counting arcs counts a corner several
times over. The `.trh` layout is documented in `trackline::read`.
"""
import sys, math, glob, statistics as st, struct, zipfile


def centreline(data):
    p = data.find(b"asphalt\0")
    if p < 4:
        return None
    table = p - 4
    f = lambda o: struct.unpack_from("<f", data, o)[0]
    u = lambda o: struct.unpack_from("<I", data, o)[0]
    pose = table - 40
    x, z, heading, length = f(pose), f(pose+4), f(pose+8), f(pose+12)
    mats = u(table)
    at = table + 4 + mats*52
    count = u(at)
    if count == 0 or count > 8192:
        return None
    segs, total = [], 0.0
    for i in range(count):
        o = at + 4 + i*60
        g = lambda k: f(o + k*4)
        seg = dict(length=g(1), radius=g(2), angle=abs(g(3)), elev=g(4), at=g(5),
                   x=g(8), z=g(11), heading=math.atan2(g(7), g(6)))
        if abs(seg["at"] - total) > 0.5:
            return None
        total += seg["length"]
        segs.append(seg)
    return dict(start=(x, z), heading=heading, length=length, segments=segs)

def from_zip(path):
    out = {}
    with zipfile.ZipFile(path) as z:
        for n in z.namelist():
            if n.lower().endswith(".trh"):
                lap = centreline(z.read(n))
                if lap:
                    out[n] = lap
    return out


JOIN = 10.0     # a run shorter than this does not separate two corners
SWEEP = 300.0   # over this radius it is a run with a bend in it

def to_segs(lap):
    out = []
    for s in lap["segments"]:
        r, L, a = s["radius"], s["length"], s["angle"]
        if abs(r) < 1e-3:
            out.append(dict(kind="straight", length=L, radius=0.0, angle=0.0))
        else:
            out.append(dict(kind="arc", length=L, radius=r, angle=a))
    return out

def corners(segs):
    """Same-sign turning of a corner radius, with only short runs let in."""
    out, cur, gap = [], None, 0.0
    for s in segs:
        turning = s["kind"] == "arc" and abs(s["radius"]) <= SWEEP
        if turning:
            sign = 1 if s["radius"] > 0 else -1
            if cur and cur["sign"] == sign and gap <= JOIN:
                cur["arcs"].append(s); cur["run"] += gap
            else:
                if cur: out.append(cur)
                cur = dict(sign=sign, arcs=[s], run=0.0, before=gap)
            gap = 0.0
        else:
            gap += s["length"]
    if cur: out.append(cur)
    for c in out:
        c["angle"] = sum(a["angle"] for a in c["arcs"])
        c["length"] = sum(a["length"] for a in c["arcs"]) + c["run"]
        c["apex"] = min(abs(a["radius"]) for a in c["arcs"])
        c["n"] = len(c["arcs"])
    return out

def runs(segs):
    """Ground between corners: straights and sweepers, merged."""
    out, run = [], 0.0
    for s in segs:
        if s["kind"] == "straight" or abs(s["radius"]) > SWEEP:
            run += s["length"]
        else:
            if run > JOIN: out.append(run)
            run = 0.0
    if run > JOIN: out.append(run)
    return out

def q(v, k):
    if not v: return 0.0
    v = sorted(v)
    i = min(len(v) - 1, max(0, int(round((len(v) - 1) * k / 100))))
    return v[i]

def report(name, segs, bbox="?"):
    lap = sum(s["length"] for s in segs)
    turning = [s for s in segs if s["kind"] == "arc" and abs(s["radius"]) <= SWEEP]
    arc = sum(s["length"] for s in turning)
    turn = sum(s["angle"] for s in segs if s["kind"] == "arc")
    cs = [c for c in corners(segs) if c["angle"] >= 25.0]
    rs = runs(segs)
    ang = [c["angle"] for c in cs]; apx = [c["apex"] for c in cs]; cln = [c["length"] for c in cs]
    print(f"\n{name}")
    print(f"  lap {lap:6.0f} m  plot {bbox:>9}  segs {len(segs):3d}  turning {arc/lap:.2f}  "
          f"gross turn {turn:5.0f}°")
    print(f"  corners {len(cs):3d} ({len(cs)/lap*1000:4.1f}/km)  arcs/corner p50 "
          f"{st.median([c['n'] for c in cs]):.0f}  "
          f"angle {q(ang,10):.0f}–{q(ang,90):.0f}° (p50 {st.median(ang):.0f})")
    print(f"  apex R {q(apx,10):4.1f}–{q(apx,90):5.1f} m (p50 {st.median(apx):.1f})   "
          f"corner run {q(cln,10):.0f}–{q(cln,90):.0f} m (p50 {st.median(cln):.0f})")
    print(f"  between corners: {len(rs)} runs, p50 {st.median(rs) if rs else 0:.0f} m, "
          f"p90 {q(rs,90):.0f}, longest {max(rs) if rs else 0:.0f}   "
          f">40 m {sum(1 for b in rs if b>40)}  >60 m {sum(1 for b in rs if b>60)}")
    return dict(name=name, lap=lap, arcfrac=arc/lap, turn=turn, nc=len(cs),
                perkm=len(cs)/lap*1000, apex=st.median(apx), ang=st.median(ang),
                narc=st.median([c['n'] for c in cs]), clen=st.median(cln),
                run=st.median(rs) if rs else 0, longest=max(rs) if rs else 0,
                over60=sum(1 for b in rs if b>60), bbox=bbox)

def main():
    import os
    if len(sys.argv) > 1:
        paths = sys.argv[1:]
    else:
        root = os.path.expanduser("~/Projects/pkz")
        paths = sorted(glob.glob(root + '/tracks/*.zip') + glob.glob(root + '/arl_new/*.zip'))
    rows = []
    for path in paths:
        for n, lap in from_zip(path).items():
            xs=[s["x"] for s in lap["segments"]]; zs=[s["z"] for s in lap["segments"]]
            # One `.pkz` carries a second, much longer lap; it is not a motocross track.
            if lap["length"] > 4000:
                continue
            rows.append(report(path.split('/')[-1].rsplit('.', 1)[0], to_segs(lap),
                               f"{max(xs)-min(xs):.0f}x{max(zs)-min(zs):.0f}"))
    if not rows:
        print("no .trh centreline found — is ~/Projects/pkz there?", file=sys.stderr)
        return
    print(f"\n=== the corpus, {len(rows)} laps ===")
    for k, label in [("lap", "lap, m"), ("arcfrac", "turning share"),
                     ("turn", "gross turn, deg"), ("nc", "corners"),
                     ("perkm", "corners per km"), ("apex", "apex radius, m"),
                     ("ang", "a corner, deg"), ("narc", "arcs in a corner"),
                     ("clen", "ground per corner, m"), ("run", "run between, m"),
                     ("longest", "longest run, m"), ("over60", "runs over 60 m")]:
        v = [r[k] for r in rows]
        print(f"  {label:22} min {min(v):7.1f}   p50 {st.median(v):7.1f}   max {max(v):7.1f}")


main()
