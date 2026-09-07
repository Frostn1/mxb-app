#!/usr/bin/env python3
"""A lap grown one piece at a time, and docked back onto its own start.

Every skeleton before this one had a *shape* — a ring, a ribbon with a return leg round the
outside — and the shape is what looked wrong. A real motocross track has no return leg: it
wanders over its ground and eventually arrives back at the gate, and the way it gets there is
by having been steered, not by having a corridor reserved for it.

So this steers. It grows a lap piece by piece, refusing any piece that would put the track
through ground it has already used, preferring the direction with the most room left in it,
and when the distance budget runs down it computes the shortest pair of turns and a straight
that lands exactly on the start pose — a Dubins path — and takes it if it is clear.

Conventions are the app's, so nothing has to be converted: heading is measured with
`heading_vector(h) = (sin h, cos h)`, a positive radius turns right, and an arc of signed
radius r through angle a moves the pose by `r*(cos h0 - cos h1), r*(sin h1 - sin h0)`.
"""
import json, math, random, sys

TAU = math.tau


def right_vector(h):
    return (math.cos(h), -math.sin(h))


def advance(pose, seg):
    """Where a segment leaves you, in the app's own frame."""
    x, z, h = pose
    if seg["kind"] == "straight":
        return (x + math.sin(h) * seg["length"], z + math.cos(h) * seg["length"], h)
    r = seg["radius"]
    a = math.radians(seg["angle"]) * (1.0 if r > 0 else -1.0)
    nh = h + a
    return (x + r * (math.cos(h) - math.cos(nh)), z + r * (math.sin(nh) - math.sin(h)), nh)


def samples(pose, seg, step=3.0):
    """The ground a segment covers: `(x, z, how far along the segment)`.

    Each point carries its own distance, not the segment's. Giving a whole segment one age is
    what made every walk paint itself in on the first move: the piece just laid ends where the
    next one starts, so if its far end is dated from its own beginning it is still "old" track
    sitting right under the new piece, and nothing is ever legal.
    """
    out = []
    if seg["kind"] == "straight":
        n = max(1, int(seg["length"] / step))
        for k in range(1, n + 1):
            t = seg["length"] * k / n
            out.append((pose[0] + math.sin(pose[2]) * t, pose[1] + math.cos(pose[2]) * t, t))
    else:
        r, ang = seg["radius"], math.radians(seg["angle"])
        n = max(1, int(abs(r) * ang / step))
        for k in range(1, n + 1):
            a = ang * k / n * (1.0 if r > 0 else -1.0)
            nh = pose[2] + a
            out.append((pose[0] + r * (math.cos(pose[2]) - math.cos(nh)),
                        pose[1] + r * (math.sin(nh) - math.sin(pose[2])),
                        abs(r) * ang * k / n))
    return out


def dubins(start, goal, r):
    """The four turn-straight-turn ways from one pose to another, shortest first.

    Returned as segment lists in the app's own form. Only the CSC families: with a lap this
    size the RLR and LRL cases only matter when the two poses are almost on top of each other,
    and a lap that close to its own start has already finished.
    """
    out = []
    for s1 in (1.0, -1.0):          # +1 turns right out of the start
        for s2 in (1.0, -1.0):
            c1 = (start[0] + s1 * r * right_vector(start[2])[0],
                  start[1] + s1 * r * right_vector(start[2])[1])
            c2 = (goal[0] + s2 * r * right_vector(goal[2])[0],
                  goal[1] + s2 * r * right_vector(goal[2])[1])
            dx, dz = c2[0] - c1[0], c2[1] - c1[1]
            d = math.hypot(dx, dz)
            if d < 1e-6:
                continue
            if s1 == s2:
                # Same handedness: the straight is parallel to the line of centres.
                theta = math.atan2(dx, dz)          # app frame: heading of that line
                tangent_h = theta
                run = d
            else:
                # Opposite: the straight crosses between them, and only if they are far
                # enough apart to have a common internal tangent.
                if d < 2.0 * r:
                    continue
                theta = math.atan2(dx, dz)
                alpha = math.acos(min(1.0, 2.0 * r / d))
                tangent_h = theta + (alpha if s1 > 0 else -alpha)
                run = math.sqrt(max(0.0, d * d - 4.0 * r * r))
            # How far each end has to turn to line up with the straight.
            a1 = ((tangent_h - start[2]) * s1) % TAU
            a2 = ((goal[2] - tangent_h) * s2) % TAU
            segs = []
            if a1 > 1e-4:
                segs.append({"kind": "arc", "radius": r * s1,
                             "angle": math.degrees(a1), "rise": 0.0})
            if run > 0.5:
                segs.append({"kind": "straight", "length": run, "rise": 0.0})
            if a2 > 1e-4:
                segs.append({"kind": "arc", "radius": r * s2,
                             "angle": math.degrees(a2), "rise": 0.0})
            length = a1 * r + run + a2 * r
            out.append((length, segs))
    out.sort(key=lambda t: t[0])
    return out


class Ground:
    """What the lap has used, on a coarse grid, so "is this clear" is cheap."""

    def __init__(self, plot, cell=8.0):
        self.cell = cell
        self.n = int(plot / cell) + 1
        self.used = [[] for _ in range(self.n * self.n)]
        self.plot = plot

    def _cells(self, x, z, reach):
        c0 = max(0, int((x - reach) / self.cell))
        c1 = min(self.n - 1, int((x + reach) / self.cell))
        r0 = max(0, int((z - reach) / self.cell))
        r1 = min(self.n - 1, int((z + reach) / self.cell))
        for r in range(r0, r1 + 1):
            for c in range(c0, c1 + 1):
                yield r * self.n + c

    def add(self, pts, at):
        for x, z, along in pts:
            i = int(z / self.cell) * self.n + int(x / self.cell)
            if 0 <= i < len(self.used):
                self.used[i].append((x, z, at + along))

    def nearest(self, x, z, ignore_after, reach=60.0, ignore_before=-1.0):
        """Distance to the closest track laid between `ignore_before` and `ignore_after`.

        The second bound is for the docking path: a lap arriving back at its gate necessarily
        comes close to the straight it left on, and without leave to ignore that, no way home
        is ever clear and the walk runs past its budget for ever.
        """
        best = reach
        for i in self._cells(x, z, reach):
            for px, pz, age in self.used[i]:
                if age > ignore_after or age < ignore_before:
                    continue
                d = math.hypot(px - x, pz - z)
                if d < best:
                    best = d
        return best

    def room(self, x, z, ignore_after):
        """How open the ground is here — used to steer towards what has not been ridden."""
        edge = min(x, z, self.plot - x, self.plot - z)
        return min(self.nearest(x, z, ignore_after), edge)


def grow(rng, plot, width, want_m):
    """Walk a lap out of the ground and dock it back onto its own start.

    Each step offers the same handful of moves — hold the line, ease left or right, turn hard
    either way — and the one taken is whichever leaves the track in the most open ground while
    clearing everything already laid. When the budget runs down, the shortest Dubins path back
    to the start pose that is also clear closes the lap exactly, which is why there is no
    return leg to route round anything.
    """
    clear = width + 8.0          # how near the lap may come to itself
    margin = 40.0
    gate_room = 78.0        # the start spur stands beside the lap and needs ground
    # Three tightnesses, and the loose one is a sweeper rather than a motorway: with a band
    # running to 60 m the median corner came out at 43 m, where a published track's is 7 to
    # 30. Two of the three are inside that band and the walk prefers turns to runs, so the
    # median lands there.
    turn_r = [rng.uniform(9.0, 14.0), rng.uniform(15.0, 24.0), rng.uniform(26.0, 38.0)]
    start = (plot * 0.5, margin + gate_room, 0.0)   # facing +z, up the plot
    pose = start
    ground = Ground(plot)
    segs = []
    laid = 0.0
    # The first stretch is the start straight, and nothing may be built on it.
    opening = {"kind": "straight", "length": rng.uniform(90.0, 130.0), "rise": 0.0}
    segs.append(opening)
    ground.add(samples(pose, opening), 0.0)
    pose = advance(pose, opening)
    laid += opening["length"]

    def legal(from_pose, seg, age_cut, skip_start=-1.0):
        for x, z, _ in samples(from_pose, seg, 2.5):
            if not (margin <= x <= plot - margin and margin <= z <= plot - margin):
                return False
            if ground.nearest(x, z, age_cut, ignore_before=skip_start) < clear:
                return False
        return True

    cap = want_m * 1.7
    while laid < cap:
        # What a rider could be given next: a run, or a turn of one of three tightnesses
        # either way. Runs are what carry the lap across the ground; turns are what keep it
        # inside the plot.
        moves = [{"kind": "straight", "length": rng.uniform(30.0, 70.0), "rise": 0.0}]
        for r in turn_r:
            for side in (1.0, -1.0):
                moves.append({"kind": "arc", "radius": r * side,
                              "angle": rng.uniform(35.0, 120.0), "rise": 0.0})
        rng.shuffle(moves)
        best, best_score = None, -1e9
        for m in moves:
            # Ignore the last thirty metres of track when checking clearance: a corner is
            # allowed to come close to the run that fed it.
            if not legal(pose, m, laid - 34.0):
                continue
            end = advance(pose, m)
            # Look a little further on, so the walk does not paint itself into a corner.
            ahead = (end[0] + math.sin(end[2]) * 26.0, end[1] + math.cos(end[2]) * 26.0)
            score = (ground.room(end[0], end[1], laid - 34.0)
                     + 0.8 * ground.room(ahead[0], ahead[1], laid - 34.0))
            if m["kind"] == "arc":
                score += 6.0            # corners are the point of a motocross track
            if score > best_score:
                best, best_score = m, score
        if best is None:
            return None                 # painted in; the caller tries another seed

        ground.add(samples(pose, best), laid)
        pose = advance(pose, best)
        laid += (best["length"] if best["kind"] == "straight"
                 else abs(best["radius"]) * math.radians(best["angle"]))
        segs.append(best)
        # Once past the budget, try to close on every step until one is clear.
        if laid > want_m * 0.72:
            for _, home in dubins(pose, start, rng.choice(turn_r[1:])):
                p = pose
                ok = True
                for seg in home:
                    # The way home may run up beside the start straight — that is where it is
                    # going — so the first stretch of the lap is not an obstacle to it.
                    if not legal(p, seg, laid - 34.0, skip_start=opening["length"] + 25.0):
                        ok = False
                        break
                    p = advance(p, seg)
                if ok and all(s["kind"] != "arc" or s["angle"] < 200.0 for s in home):
                    return segs + home, start
    return None


def closes_to(segs, start):
    pose = start
    for s in segs:
        pose = advance(pose, s)
    return math.hypot(pose[0] - start[0], pose[1] - start[1])


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else random.randrange(1 << 30)
    rng = random.Random(seed)
    plot = 620.0
    width = round(rng.uniform(10.0, 13.5), 1)
    grown = None
    for _ in range(6):
        grown = grow(rng, plot, width, rng.uniform(1500.0, 2100.0))
        if grown:
            break
    if not grown:
        print(f"seed {seed}: REJECTED — the walk painted itself in", file=sys.stderr)
        sys.exit(2)
    segs, start = grown
    miss = closes_to(segs, start)
    lap = sum(s["length"] if s["kind"] == "straight"
              else abs(s["radius"]) * math.radians(s["angle"]) for s in segs)
    turn = sum(s["angle"] for s in segs if s["kind"] == "arc")
    print(f"seed {seed}: {lap:.0f} m, {sum(1 for s in segs if s['kind']=='arc')} corners, "
          f"{turn:.0f}° turn, closes to {miss:.2f} m, {len(segs)} segments", file=sys.stderr)
    if miss > 2.0:
        print(f"seed {seed}: REJECTED — misses itself by {miss:.1f} m", file=sys.stderr)
        sys.exit(2)
    # The rest of the program — jumps, ground, a name — comes from the same place it always
    # did. Only the skeleton changed.
    import importlib.util
    spec = importlib.util.spec_from_file_location(
        "layout", __file__.rsplit("/", 1)[0] + "/track-layout.py")
    lay = importlib.util.module_from_spec(spec)
    saved, sys.argv = sys.argv, ["layout"]
    try:
        spec.loader.exec_module(lay)
    except SystemExit:
        pass
    finally:
        sys.argv = saved
    prog = {
        "name": lay.NAMES[seed % len(lay.NAMES)][0] + " " + lay.NAMES[seed % len(lay.NAMES)][1],
        "author": "MXB App",
        "location": lay.PLACES[(seed // 7) % len(lay.PLACES)],
        "width": width,
        "terrain": {
            "sizeX": plot, "sizeZ": plot, "samples": 2049,
            "scale": rng.choice([63, 70]),
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
        "start": {"x": round(start[0], 2), "z": round(start[1], 2),
                  "angle": round(math.degrees(start[2]) % 360.0, 2)},
        "segments": segs,
        "features": lay.features(rng, segs),
    }
    print(f"seed {seed}: {prog['name']}, {len(prog['features'])} features "
          f"({len(prog['features'])/lap*1000:.0f}/km)", file=sys.stderr)
    json.dump(prog, sys.stdout, indent=1)


if __name__ == "__main__":
    main()
