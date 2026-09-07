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
    """The skeleton of a motocross track: a ribbon folded back and forth across the plot.

    A radius that wobbles round a centre can only draw a ring, and a ring is not what a track
    looks like. What a real one is, from above, is a set of runs across the ground joined by
    hairpins at the ends, with a way back to the gate down one side. The infield gets used and
    the shape has a top and a bottom.

    Every distance here is budgeted rather than hoped for. The runs, the bulge each hairpin
    makes past the end of them, the corridor the way home needs and the margin round the whole
    thing are laid out first, and what is left over is the length of a run. Lanes sit `gap`
    apart and bow by less than half of what is between them, so two runs cannot meet.
    """
    _ = n
    margin = 58.0
    lo, hi = margin, PLOT - margin
    # Lanes close together, and enough of them. Spread three lanes over the whole plot and
    # they sit 154 m apart, which makes every hairpin between them a 240 m arc — half the lap
    # spent turning round. A real switchback is 35 to 45 m from the run beside it, and there
    # are five or six of them.
    gap = rng.uniform(34.0, 46.0)
    lanes = rng.randint(4, 6)
    corridor = 42.0
    block = (lanes - 1) * gap
    # The block sits in the plot with the corridor below it, centred on what is left.
    run_z = lo + corridor + max(0.0, (hi - lo - corridor - block)) * 0.5
    bulge = gap * 0.5                      # how far a hairpin reaches past a run's end
    # And the runs only as long as the lap can afford: everything else is fixed, so this is
    # what sets the distance.
    want = rng.uniform(1250.0, 1750.0)
    hairpins = (lanes - 1) * math.pi * bulge
    home = block + 120.0
    run = max(120.0, (want - hairpins - home) / (lanes + 1))
    x0 = lo + 12.0
    x1 = min(hi - corridor - bulge, x0 + run)
    step = 9.0
    bow = min(gap * 0.30, (gap - 26.0) * 0.5)
    waves = [(rng.uniform(0.6, 2.0), rng.uniform(0, math.tau)) for _ in range(lanes)]
    pts = []

    for lane in range(lanes):
        z = run_z + lane * gap
        east = lane % 2 == 0
        a, b = (x0, x1) if east else (x1, x0)
        steps = max(4, int(abs(b - a) / step))
        k_wave, phase = waves[lane]
        for k in range(steps + 1):
            t = k / steps
            x = a + (b - a) * t
            pts.append((x, z + bow * math.sin(t * math.tau * k_wave + phase)))
        if lane == lanes - 1:
            break
        cz, cx = z + gap * 0.5, b
        for k in range(1, 9):
            th = math.pi * k / 9
            pts.append((cx + bulge * math.sin(th) * (1.0 if east else -1.0),
                        cz - bulge * math.cos(th)))

    # The way home: out into the corridor, down the side, along the bottom, and back up to
    # where the gate stands. The long straight along the bottom is what a start needs.
    end_east = (lanes - 1) % 2 == 0
    out_x = (hi - corridor * 0.4) if end_east else (lo + 12.0)
    home_z = lo + corridor * 0.45
    if end_east:
        pts.append((out_x, run_z + (lanes - 1) * gap))
        pts.append((out_x, home_z))
        steps = max(4, int((out_x - x0) / step))
        for k in range(1, steps + 1):
            t = k / steps
            pts.append((out_x + (x0 - out_x) * t,
                        home_z + 14.0 * math.sin(t * math.tau * 1.5)))
    else:
        pts.append((out_x, run_z + (lanes - 1) * gap))
        pts.append((out_x, home_z))
        steps = max(4, int((x1 - out_x) / step))
        for k in range(1, steps + 1):
            pts.append((out_x + (x1 - out_x) * k / steps, home_z))
    return pts


PLOT = 620.0


def simplify(pts, tol):
    """Drop points that lie on the line between their neighbours.

    The skeleton is drawn every nine metres, so most of its points sit on a straight run and
    turn a fraction of a degree. Filleting those is pointless — and skipping them, which is
    what the code did, loses their angle and leaves the lap short of itself. Take them out
    instead, so every vertex that survives is a corner worth cutting.
    """
    keep = [pts[0]]
    for p in pts[1:]:
        a = keep[-1]
        if math.hypot(p[0] - a[0], p[1] - a[1]) >= tol:
            keep.append(p)
    # Then collinear runs, by the area of the triangle three neighbours make.
    out = [keep[0]]
    for i in range(1, len(keep) - 1):
        a, b, c = out[-1], keep[i], keep[i + 1]
        area = abs((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
        base = math.hypot(c[0] - a[0], c[1] - a[1]) or 1.0
        if area / base > 0.85:      # more than 45 cm off the chord: a real corner
            out.append(b)
    out.append(keep[-1])
    return out


def fillet(pts, rng, width):
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
        # Every vertex turns, however gently. Dropping the small ones is what left the lap
        # short of itself: the path skips those degrees, fifty vertices' worth of them add up,
        # and the finish arrives tens of metres from the start. A three-degree turn is an arc
        # of a hundred-metre radius, which is a sweeper — not nothing.
        if abs(delta) < math.radians(0.4):
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
            lo, hi = 9.0, 17.0
        elif deg >= 45.0:
            lo, hi = 12.0, 24.0
        elif deg >= 22.0:
            lo, hi = 18.0, 34.0
        else:
            lo, hi = 26.0, 45.0
        # As much of the edge as the fillet can take, within the band the corner allows.
        # Published laps are 61 to 91 per cent arc: the straight between two corners is
        # what is left over, not something to leave room for.
        cap = 0.49 * min(l1, l2) / max(math.tan(half), 1e-3)
        r = min(cap, hi * rng.uniform(0.88, 1.0))
        if r < lo:
            r = min(cap, lo)
        # Never tighter than the track is wide.
        #
        # The inside of a corner of radius r is r - width/2 from its centre: at a radius under
        # half the width, that is negative and the track has folded through itself. It is also
        # what makes a "closest pass" check reject perfectly good hairpins — the two sides of
        # one are only 2r apart, and no threshold above the width can tell the two cases
        # apart. So the radius carries the rule instead.
        r = max(r, width * 0.5 + 2.0)
        r = max(r, 7.5)
        turns.append((delta, r, r * math.tan(half)))
    # Make the roundings fit the edges they share.
    #
    # Two corners either side of a short edge can each want more of it than is there. The
    # straight between them then comes out negative, both arcs are drawn anyway, and the path
    # curls back through itself — a loop in the middle of the track, which is exactly what it
    # built. Shrink both until they fit, a few times over, because shrinking a corner changes
    # the fit of the edge on its other side too.
    for _ in range(6):
        for i in range(n):
            a, b = pts[i], pts[(i + 1) % n]
            edge = math.hypot(b[0] - a[0], b[1] - a[1])
            t_here = turns[i][2] if turns[i] else 0.0
            t_next = turns[(i + 1) % n][2] if turns[(i + 1) % n] else 0.0
            want = t_here + t_next
            if want <= edge * 0.78 or want <= 1e-6:
                continue
            k = edge * 0.78 / want
            for j in (i, (i + 1) % n):
                if turns[j]:
                    delta, r, t = turns[j]
                    turns[j] = (delta, r * k, t * k)

    # Drop the corners too small to draw *before* the edges are measured.
    #
    # Skipping the arc but still taking its tangent out of the straights either side is what
    # left the lap 46 m short of itself: the path loses two tangents of length and never makes
    # the turn. Repair then shuts the gap with segments of its own, and those are a loop in
    # the middle of the track.

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
    # Straights that follow one another are one straight. Four collinear vertices emit four
    # segments of twenty metres, and a gate row needs sixty *in one run* — the lap had the
    # ground and the program did not say so.
    merged = []
    for seg in segs:
        if (seg["kind"] == "straight" and merged and merged[-1]["kind"] == "straight"):
            merged[-1] = {"kind": "straight",
                          "length": round(merged[-1]["length"] + seg["length"], 1),
                          "rise": 0.0}
        else:
            merged.append(seg)
    segs = merged

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
        if pick < 0.55 and room > 30.0:
            length = round(min(rng.uniform(26.0, 44.0), room), 1)
            out.append({"kind": "tabletop", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(2.2, 3.8), 2)})
        elif pick < 0.68 and room > 26.0:
            gap = round(rng.uniform(9.0, 16.0), 1)
            length = min(gap + 14.0, room)
            out.append({"kind": "double", "at": round(pos, 1),
                        "height": round(rng.uniform(1.1, 1.8), 2), "gap": gap})
        elif pick < 0.82 and room > 24.0:
            length = round(min(rng.uniform(18.0, 28.0), room), 1)
            out.append({"kind": "stepUp", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(1.4, 2.6), 2)})
        else:
            length = round(min(rng.uniform(10.0, 16.0), room), 1)
            if length < 8.0:
                break
            out.append({"kind": "roller", "at": round(pos, 1), "length": length,
                        "height": round(rng.uniform(0.55, 0.95), 2)})
        pos += length + rng.uniform(8.0, 20.0)
    return out


def clearance(prog):
    """The closest two distant parts of the lap come to each other, in metres.

    Star-shaped stops the outline crossing. It does not stop the *track* — ten metres of it
    either side of a centreline — passing through its own ground where two lobes of the loop
    lie close together, and it does not stop a rounding that overran its edge from curling
    into a pigtail. So the lap is walked and measured, which is the only claim worth making.
    """
    x, z = prog["start"]["x"], prog["start"]["z"]
    h = math.radians(prog["start"]["angle"])
    pts = []
    for s in prog["segments"]:
        if s["kind"] == "straight":
            n = max(1, int(s["length"] / 2))
            for _ in range(n):
                x += math.sin(h) * s["length"] / n
                z += math.cos(h) * s["length"] / n
                pts.append((x, z))
        else:
            r, a = s["radius"], math.radians(s["angle"])
            n = max(1, int(abs(r) * a / 2))
            for _ in range(n):
                h += a / n * (1 if r > 0 else -1)
                x += math.sin(h) * abs(r) * a / n
                z += math.cos(h) * abs(r) * a / n
                pts.append((x, z))
    # Does it cross itself at all? Unambiguous, unlike a distance: a hairpin comes close to
    # itself and never crosses, a pigtail crosses. This is what the width-based check missed —
    # it only compares points twenty-four metres apart round the lap, and a loop ten metres
    # across is smaller than that.
    def hits(p1, p2, p3, p4):
        def side(a, b, c):
            return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        d1, d2 = side(p3, p4, p1), side(p3, p4, p2)
        d3, d4 = side(p1, p2, p3), side(p1, p2, p4)
        return ((d1 > 0) != (d2 > 0)) and ((d3 > 0) != (d4 > 0))

    for i in range(len(pts) - 1):
        for j in range(i + 2, len(pts) - 1):
            if i == 0 and j == len(pts) - 2:
                continue
            if hits(pts[i], pts[i + 1], pts[j], pts[j + 1]):
                return -1.0

    worst = 1e9
    n = len(pts)
    for i in range(n):
        for j in range(i + 12, n):
            # Round the lap in *either* direction. A lap is a loop, so its finish is next to
            # its start: measuring only forwards, the closing seam came back as the closest
            # pass on every seed and every one was thrown out for being a track — 4 m from
            # itself at sample 0 and sample 600, which is the same piece of ground.
            if min(j - i, n - (j - i)) < 12:
                continue
            d = math.hypot(pts[i][0] - pts[j][0], pts[i][1] - pts[j][1])
            if d < worst:
                worst = d
    return worst


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else random.randrange(1 << 30)
    rng = random.Random(seed)
    width = round(rng.uniform(10.0, 13.5), 1)
    segs, (sx, sz, heading) = fillet(simplify(outline(rng, 0), 9.0), rng, width)
    length = sum(s["length"] if s["kind"] == "straight"
                 else abs(s["radius"]) * math.radians(s["angle"]) for s in segs)
    feats = features(rng, segs)
    first, second = NAMES[seed % len(NAMES)]
    prog = {
        "name": f"{first} {second}",
        "author": "MXB App",
        "location": PLACES[(seed // 7) % len(PLACES)],
        "width": width,
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
    # Does it come back to where it started? Walked, not assumed.
    x, z = prog["start"]["x"], prog["start"]["z"]
    h = math.radians(prog["start"]["angle"])
    for seg in prog["segments"]:
        if seg["kind"] == "straight":
            x += math.sin(h) * seg["length"]
            z += math.cos(h) * seg["length"]
        else:
            # The app's own walk: heading runs with the *signed* radius, and the chord comes
            # from the difference of the two headings.
            r = seg["radius"]
            a = math.radians(seg["angle"]) * (1.0 if r > 0 else -1.0)
            nh = h + a
            x += r * (math.cos(h) - math.cos(nh))
            z += r * (math.sin(nh) - math.sin(h))
            h = nh
    miss = math.hypot(x - prog["start"]["x"], z - prog["start"]["z"])
    if miss > 6.0:
        print(f"seed {seed}: REJECTED — the lap misses itself by {miss:.0f} m",
              file=sys.stderr)
        sys.exit(2)

    # Measured, not predicted: what the hairpins and the way home actually add is more than
    # the budget the runs were cut to.
    lap_m = sum(seg["length"] if seg["kind"] == "straight"
                else abs(seg["radius"]) * math.radians(seg["angle"])
                for seg in prog["segments"])
    if not 900.0 <= lap_m <= 2450.0:
        print(f"seed {seed}: REJECTED — the lap is {lap_m:.0f} m; published tracks run "
              f"500–2600 and this wants headroom for what repair adds", file=sys.stderr)
        sys.exit(2)
    if len(prog["segments"]) > 190:
        print(f"seed {seed}: REJECTED — {len(prog['segments'])} segments, and 200 is the most "
              f"a published track carries", file=sys.stderr)
        sys.exit(2)

    longest = max((seg["length"] for seg in prog["segments"]
                   if seg["kind"] == "straight"), default=0.0)
    if longest < 62.0:
        print(f"seed {seed}: REJECTED — its longest straight is {longest:.0f} m and a gate "
              f"row needs about 60", file=sys.stderr)
        sys.exit(2)

    gap = clearance(prog)
    if gap < 0.0:
        print(f"seed {seed}: REJECTED — the lap crosses itself", file=sys.stderr)
        sys.exit(2)
    if gap < prog["width"] + 1.0:
        print(f"seed {seed}: REJECTED — the lap passes within {gap:.1f} m of itself, "
              f"and it is {prog['width']:.0f} m wide", file=sys.stderr)
        sys.exit(2)
    print(json.dumps(prog, indent=1))
    corners = sum(1 for s in segs if s["kind"] == "arc")
    total = sum(s["angle"] for s in segs if s["kind"] == "arc")
    print(f"seed {seed}: {prog['name']}, {length:.0f} m, {corners} corners, "
          f"{total:.0f}° total turn, {len(feats)} features, {gap:.0f} m clear, "
          f"closes to {miss:.1f} m, start {longest:.0f} m, "
          f"({len(feats)/max(length,1)*1000:.0f}/km)", file=sys.stderr)

main()
