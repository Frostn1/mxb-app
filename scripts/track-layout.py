#!/usr/bin/env python3
"""A lap, from a seed. Stands in for the model while track generation is down.

The shape is a closed loop whose radius wobbles with the angle round its centre — star
shaped, so it can never cross itself, and closed because it goes all the way round. Its
corners are then filleted into arcs, which is what a `.tcl` is made of. Wobble the radius
hard enough and the loop turns both ways, which is where the 1400-3600 degrees of total
turning a published lap carries comes from: the net is one circle, the total is not.
"""
import json, math, random, sys

NAMES = [
    ("Ashgrove", "National"), ("Blackpine", "Park"), ("Cold Harbour", "MX"),
    ("Draycott", "Valley"), ("Eastmoor", "Raceway"), ("Fallow Hill", "MX"),
    ("Greystone", "National"), ("Hollowbrook", "Park"), ("Ironbark", "Raceway"),
    ("Juniper Flats", "MX"), ("Kestrel Ridge", "National"), ("Long Marsh", "Park"),
    ("Millbrook", "MX"), ("Northgate", "Raceway"), ("Oakhanger", "National"),
]
PLACES = ["Somerset", "Wexford", "Alberta", "Nord-Pas", "Lombardy", "Otago",
          "Jutland", "Kalmar", "Wallonia", "Aragon", "Limburg", "Waikato"]

CENTRE = 310.0            # of a 620 m plot
TIGHT_M = 22.0            # under this, a corner is no place for a takeoff
R_MIN, R_MAX = 9.0, 28.0  # corner radii the corpus allows


def outline(rng, n):
    """`n` points round a centre, at a radius that wobbles with the angle."""
    # Wobbled hard on purpose. A gentle loop turns 360 degrees in total and a published lap
    # turns 1400 to 3600: the difference is corners that go the other way, and those only
    # exist where the radius swings far enough in and out to bend the loop back on itself.
    base = rng.uniform(185.0, 225.0)
    waves = [(rng.randint(3, 6), rng.uniform(0.26, 0.40), rng.uniform(0, math.tau)),
             (rng.randint(6, 10), rng.uniform(0.16, 0.28), rng.uniform(0, math.tau)),
             (rng.randint(9, 13), rng.uniform(0.06, 0.14), rng.uniform(0, math.tau))]
    pts = []
    for i in range(n):
        th = math.tau * i / n
        r = base * (1.0 + sum(a * math.sin(k * th + p) for k, a, p in waves))
        r = max(60.0, r)
        pts.append((CENTRE + r * math.cos(th), CENTRE + r * math.sin(th)))
    return pts


def fillet(pts, rng):
    """Round every corner of the polygon into an arc, and keep what is left as straights."""
    n = len(pts)
    turns = []
    for i in range(n):
        a, b, c = pts[i - 1], pts[i], pts[(i + 1) % n]
        v1 = (b[0] - a[0], b[1] - a[1])
        v2 = (c[0] - b[0], c[1] - b[1])
        l1 = math.hypot(*v1) or 1e-6
        l2 = math.hypot(*v2) or 1e-6
        cross = v1[0] * v2[1] - v1[1] * v2[0]
        dot = v1[0] * v2[0] + v1[1] * v2[1]
        delta = math.atan2(cross, dot)          # signed turn at this vertex
        if abs(delta) < math.radians(6.0):
            turns.append(None)
            continue
        half = abs(delta) / 2.0
        # The fillet cannot eat more than half of either edge beside it.
        # Radius by how hard the corner is. A hairpin is 9 to 20 m and a sweeper is a
        # hundred, and using the hairpin's radius for every bend is what leaves a lap
        # reading as straights joined by corners: an arc of radius 25 through 15 degrees
        # is seven metres long, and the forty metres either side of it are straight.
        deg = abs(math.degrees(delta))
        if deg >= 80.0:
            lo, hi = 9.0, 19.0
        elif deg >= 45.0:
            lo, hi = 15.0, 30.0
        elif deg >= 22.0:
            lo, hi = 28.0, 60.0
        else:
            lo, hi = 55.0, 130.0
        # As much of the edge as the fillet can take, within the band the corner allows.
        # Published laps are 61 to 91 per cent arc: the straight between two corners is
        # what is left over, not something to leave room for.
        cap = 0.49 * min(l1, l2) / max(math.tan(half), 1e-3)
        r = min(cap, hi * rng.uniform(0.88, 1.0))
        if r < lo:
            r = min(cap, lo)
        r = max(r, 7.5)
        turns.append((delta, r, r * math.tan(half)))
    segs = []
    for i in range(n):
        a, b = pts[i], pts[(i + 1) % n]
        edge = math.hypot(b[0] - a[0], b[1] - a[1])
        t_here = turns[i][2] if turns[i] else 0.0
        t_next = turns[(i + 1) % n][2] if turns[(i + 1) % n] else 0.0
        run = edge - t_here - t_next
        if run > 1.0:
            segs.append({"kind": "straight", "length": round(run, 1), "rise": 0.0})
        t = turns[(i + 1) % n]
        if t:
            delta, r, _ = t
            # Positive radius turns right, and screen-space positive cross is a left turn.
            segs.append({"kind": "arc",
                         "radius": round(-r if delta > 0 else r, 1),
                         "angle": round(abs(math.degrees(delta)), 1),
                         "rise": 0.0})
    # Where the lap begins: the first tangent point, running along the first edge.
    a, b = pts[0], pts[1]
    t0 = turns[0][2] if turns[0] else 0.0
    l = math.hypot(b[0] - a[0], b[1] - a[1]) or 1.0
    sx = a[0] + (b[0] - a[0]) / l * t0
    sz = a[1] + (b[1] - a[1]) / l * t0
    heading = math.degrees(math.atan2(b[0] - a[0], b[1] - a[1])) % 360.0
    return segs, (round(sx, 2), round(sz, 2), round(heading, 2))


def features(rng, segs):
    """Jumps down the lap, by distance round it rather than by which segment they land on.

    Published tracks put them everywhere: Indiana is 109 arcs to 11 straights and still
    carries between twelve and forty-five lips per kilometre, so most of its jumps are in
    or beside corners. Placing them on straights only means a lap made of corners — which
    is what a lap is — comes out with nothing built on it.
    """
    total = 0.0
    spans = []          # (from, to, radius) — radius is None on a straight
    for s in segs:
        run = (s["length"] if s["kind"] == "straight"
               else abs(s["radius"]) * math.radians(s["angle"]))
        spans.append((total, total + run,
                      None if s["kind"] == "straight" else abs(s["radius"])))
        total += run

    def tight(at, length):
        """Is any of this feature inside a corner too tight to jump out of?"""
        for a, b, r in spans:
            if r is not None and r < TIGHT_M and b > at and a < at + length:
                return True
        return False

    out = []
    # The first stretch is where the gate row goes, and forty riders arrive at the first
    # jump in a pack. Leave it bare.
    pos = 130.0
    while pos < total - 40.0:
        room = total - 20.0 - pos
        if tight(pos, 34.0):
            pos += 12.0
            continue
        pick = rng.random()
        if pick < 0.28 and room > 22.0:
            length = round(min(rng.uniform(18.0, 32.0), room), 1)
            out.append({"kind": "tabletop", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(1.1, 2.7), 2)})
        elif pick < 0.56 and room > 11.0:
            length = round(min(rng.uniform(9.0, 15.0), room), 1)
            out.append({"kind": "roller", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(0.45, 0.9), 2)})
        elif pick < 0.74 and room > 24.0:
            count = rng.randint(4, 8)
            spacing = round(rng.uniform(3.6, 5.4), 1)
            length = count * spacing
            if length > room:
                count = max(3, int(room / spacing))
                length = count * spacing
            out.append({"kind": "whoops", "at": round(pos, 1), "count": count,
                        "spacing": spacing, "height": round(rng.uniform(0.4, 0.7), 2)})
        elif pick < 0.9 and room > 22.0:
            length = round(min(rng.uniform(15.0, 24.0), room), 1)
            out.append({"kind": "stepUp", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(1.0, 2.2), 2)})
        else:
            length = round(min(rng.uniform(12.0, 18.0), room), 1)
            if length < 8.0:
                break
            out.append({"kind": "roller", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(0.5, 0.8), 2)})
        pos += length + rng.uniform(12.0, 34.0)
    return out


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else random.randrange(1 << 30)
    rng = random.Random(seed)
    segs, (sx, sz, heading) = fillet(outline(rng, rng.randint(60, 78)), rng)
    length = sum(s["length"] if s["kind"] == "straight"
                 else abs(s["radius"]) * math.radians(s["angle"]) for s in segs)
    feats = features(rng, segs)
    first, second = NAMES[seed % len(NAMES)]
    prog = {
        "name": f"{first} {second}",
        "author": "MXB App",
        "location": PLACES[(seed // 7) % len(PLACES)],
        "width": round(rng.uniform(10.0, 13.5), 1),
        "terrain": {
            "sizeX": 620, "sizeZ": 620, "samples": 2049,
            "scale": rng.choice([50, 63, 70]),
            "relief": {
                "amplitude": round(rng.uniform(5.0, 11.0), 1),
                "wavelength": rng.choice([320, 380, 420, 480]),
                "seed": seed % 9973, "texture": 0.085,
                "tilt": round(rng.uniform(10.0, 30.0), 1),
                "tiltAngle": round(rng.uniform(0.0, 359.0), 1),
                "landforms": rng.randint(4, 8),
                "landformHeight": round(rng.uniform(10.0, 22.0), 1),
            },
        },
        "start": {"x": sx, "z": sz, "angle": heading},
        "segments": segs,
        "features": feats,
    }
    print(json.dumps(prog, indent=1))
    corners = sum(1 for s in segs if s["kind"] == "arc")
    total = sum(s["angle"] for s in segs if s["kind"] == "arc")
    print(f"seed {seed}: {prog['name']}, {length:.0f} m, {corners} corners, "
          f"{total:.0f}° total turn, {len(feats)} features "
          f"({len(feats)/max(length,1)*1000:.0f}/km)", file=sys.stderr)

main()
