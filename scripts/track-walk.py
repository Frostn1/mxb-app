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


class Cover:
    """Which parts of the plot the lap has been near, on a coarse grid.

    The walk used to be scored by how much open ground lay ahead of it, and the most open
    ground is always the perimeter — so it followed the boundary round and left the middle
    empty, which from above is a giant ring. Rewarding *new ground covered* instead makes it
    fill the plot, because a move into the middle scores and a move along the edge it has
    already been down does not.
    """

    def __init__(self, plot, cell=26.0):
        self.cell = cell
        self.n = int(plot / cell) + 1
        self.seen = [False] * (self.n * self.n)

    def index(self, x, z):
        c, r = int(x / self.cell), int(z / self.cell)
        if 0 <= c < self.n and 0 <= r < self.n:
            return r * self.n + c
        return None

    def fresh(self, pts):
        """How many cells this piece would visit that nothing has visited yet."""
        hit = set()
        for x, z, _ in pts:
            i = self.index(x, z)
            if i is not None and not self.seen[i]:
                hit.add(i)
        return len(hit)

    def add(self, pts):
        for x, z, _ in pts:
            i = self.index(x, z)
            if i is not None:
                self.seen[i] = True


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


# How much of a lap has to be tight enough to wear a rut. A hand-written track that measures
# like Indiana carries about this much under a 14 m radius.
TIGHT_SHARE = 0.10


def grow(rng, plot, width, want_m):
    """Walk a lap out of the ground and dock it back onto its own start.

    Each step offers the same handful of moves — hold the line, ease left or right, turn hard
    either way — and the one taken is whichever leaves the track in the most open ground while
    clearing everything already laid. When the budget runs down, the shortest Dubins path back
    to the start pose that is also clear closes the lap exactly, which is why there is no
    return leg to route round anything.
    """
    clear = width + 5.0          # how near the lap may come to itself
    margin = 26.0
    gate_room = 78.0        # the start spur stands beside the lap and needs ground
    # Three tightnesses, and the tightest is a hairpin.
    #
    # Ruts reach full depth at a 14 m radius and start forming at 40 — which is measured, and
    # is the whole reason a corner wears and a sweeper does not. The first laps out of this
    # walk put one per cent of their length under 14 m against a hand-written track's eight,
    # and rode, in one word, with no ruts. Not even in the turns: at 20 to 30 m the ground
    # barely digs.
    turn_r = [rng.uniform(8.5, 12.5), rng.uniform(14.0, 20.0), rng.uniform(24.0, 34.0)]
    start = (plot * 0.5, margin + gate_room, 0.0)   # facing +z, up the plot
    pose = start
    ground = Ground(plot)
    cover = Cover(plot)
    segs = []
    laid = 0.0
    # The first stretch is the start straight, and nothing may be built on it.
    opening = {"kind": "straight", "length": rng.uniform(90.0, 130.0), "rise": 0.0}
    tight_m = 0.0
    segs.append(opening)
    ground.add(samples(pose, opening), 0.0)
    cover.add(samples(pose, opening))
    pose = advance(pose, opening)
    laid += opening["length"]

    def legal(from_pose, seg, age_cut, near_gate=0.0):
        for x, z, _ in samples(from_pose, seg, 2.5):
            if not (margin <= x <= plot - margin and margin <= z <= plot - margin):
                return False
            # Arriving at the gate is allowed to be close to the gate, and to nothing else.
            #
            # Exempting the first stretch of the *lap* instead let the way home drive
            # straight through the first corner — "segment 1 runs within 0 m of segment 44" —
            # because that corner is early, not because it is near the finish. The exemption
            # has to be about where a piece is, not when it was laid.
            if near_gate > 0.0 and math.hypot(x - start[0], z - start[1]) < near_gate:
                continue
            if ground.nearest(x, z, age_cut) < clear:
                return False
        return True

    cap = want_m * 1.7
    while laid < cap:
        # What a rider could be given next: a run, or a turn of one of three tightnesses
        # either way. Runs are what carry the lap across the ground; turns are what keep it
        # inside the plot.
        # A straight, unless the lap is already on one long enough. Consecutive straights are
        # colinear, so the app's `straight_runs` reads a row of them as ONE straight — and a
        # row of 35-80 m moves reached 248 m against the 125 m cap (`corpus::STRAIGHT_M`, the
        # FFM's limit and the only one any federation writes). Indiana's longest is 62 m.
        running = 0.0
        for prev in reversed(segs):
            if prev["kind"] != "straight":
                break
            running += prev["length"]
        moves = []
        room_on_the_straight = 125.0 - running
        if room_on_the_straight > 35.0:
            moves.append({"kind": "straight",
                          "length": rng.uniform(35.0, min(80.0, room_on_the_straight)),
                          "rise": 0.0})
        # The lap's own wander, and most of what it is made of. A published track is a chain
        # of arcs — Indiana runs 109 of them against 11 straights — but they average twenty
        # degrees apiece, not a hundred. Without this move every piece of the lap was a real
        # corner, which is the slalom: twenty-six arcs averaging 96 degrees, flipping
        # direction fifteen times.
        for _ in range(3):
            r = rng.uniform(48.0, 160.0)
            for side in (1.0, -1.0):
                moves.append({"kind": "arc", "radius": r * side,
                              "angle": rng.uniform(11.0, 32.0), "rise": 0.0})
        for r in turn_r:
            for side in (1.0, -1.0):
                # A hairpin turns most of the way round. Getting the tight ground a lap needs
                # out of many small tight corners costs a corner apiece — forty of them,
                # where a published track carries ten to thirty — while three real hairpins
                # carry the same metres and read as corners a rider remembers.
                ang = rng.uniform(120.0, 178.0) if r < 14.0 else rng.uniform(55.0, 108.0)
                moves.append({"kind": "arc", "radius": r * side,
                              "angle": ang, "rise": 0.0})
        rng.shuffle(moves)
        # No corner straight into the opposite corner. That is what a slalom is, and it is
        # what the ground between them cannot carry: a rider needs somewhere to stand the
        # bike up. Two gentle bends may still answer each other, because that is a lap
        # wandering rather than a rider being thrown from edge to edge.
        prev = segs[-1] if segs else None
        def slaloms(m):
            if prev is None or prev["kind"] != "arc" or m["kind"] != "arc":
                return False
            if (prev["radius"] > 0.0) == (m["radius"] > 0.0):
                return False
            return max(prev["angle"], m["angle"]) >= 38.0

        best, best_score = None, -1e9
        for m in moves:
            if slaloms(m):
                continue
            # Ignore the last thirty metres of track when checking clearance: a corner is
            # allowed to come close to the run that fed it.
            if not legal(pose, m, laid - 34.0):
                continue
            end = advance(pose, m)
            # Ground it would be the first to visit, which is what makes a lap fill its plot
            # rather than circle it.
            # Per metre travelled, not per move: rewarding raw coverage buys it with long
            # straights, because a hundred metres of run touches more ground than a corner
            # ever will, and a lap of long runs with angles between them is not a track.
            run_m = (m["length"] if m["kind"] == "straight"
                     else abs(m["radius"]) * math.radians(m["angle"]))
            score = 190.0 * cover.fresh(samples(pose, m, 6.0)) / max(run_m, 1.0)
            # Room still counts, but only enough to keep the walk from painting itself in.
            ahead = (end[0] + math.sin(end[2]) * 26.0, end[1] + math.cos(end[2]) * 26.0)
            score += 0.35 * ground.room(ahead[0], ahead[1], laid - 34.0)
            if m["kind"] == "arc":
                score += 3.0            # corners are the point, but ten to thirty of them
                # And a lap needs its share of ground tight enough to wear. Until it has
                # that, a hairpin outscores anything the open ground can offer.
                if abs(m["radius"]) < 14.0:
                    # In metres, like the room term it competes with: at four it was noise.
                    score += 45.0 if tight_m < laid * TIGHT_SHARE else 1.0
            if score > best_score:
                best, best_score = m, score
        if best is None:
            return None                 # painted in; the caller tries another seed

        ground.add(samples(pose, best), laid)
        cover.add(samples(pose, best))
        pose = advance(pose, best)
        run = (best["length"] if best["kind"] == "straight"
               else abs(best["radius"]) * math.radians(best["angle"]))
        if best["kind"] == "arc" and abs(best["radius"]) < 14.0:
            tight_m += run
        laid += run
        segs.append(best)
        # Once past the budget, try to close on every step until one is clear.
        if laid > want_m * 0.72:
            for _, home in dubins(pose, start, rng.choice(turn_r[1:])):
                # Walked, not trusted. One of the four families has a sign in it that only
                # bites on some geometries, and the symptom is a lap that misses itself by
                # thirty metres — which the repair pass then shuts with segments of its own,
                # and those are the loop this whole approach exists to avoid. A way home that
                # does not land is simply not a candidate.
                landed = pose
                for seg in home:
                    landed = advance(landed, seg)
                if math.hypot(landed[0] - start[0], landed[1] - start[1]) > 0.25:
                    continue
                p = pose
                ok = True
                for seg in home:
                    # The way home may run up beside the start straight — that is where it is
                    # going — so the first stretch of the lap is not an obstacle to it.
                    if not legal(p, seg, laid - 34.0, near_gate=opening["length"] * 0.8):
                        ok = False
                        break
                    p = advance(p, seg)
                # And no straight anywhere on the finished lap may run past the cap.
                #
                # Checked on the WHOLE lap rather than on the way home, because two things
                # make a long straight and neither is one segment: consecutive straights are
                # colinear, so the app's `straight_runs` reads a row of them as one — and a
                # Dubins path is arc-straight-arc, so its straight sits in the MIDDLE and a
                # look at the last segment never sees it. That is what left a 244 m straight
                # on a lap whose every move was capped at 80. The limit is the FFM's 125 m,
                # the only straight-length rule any federation writes; Indiana's longest is 62.
                if ok and longest_straight(segs + home, opening) > 125.0:
                    continue
                if ok and all(s["kind"] != "arc" or s["angle"] < 200.0 for s in home):
                    return segs + home, start
    return None


def longest_straight(segs, opening):
    """The longest unbroken straight a rider meets, metres — merged the way the app merges.

    Mirrors `Station::straight_runs`: consecutive straights are one straight, and so is the
    pair either side of the finish line, because a lap that ends on a straight and begins on
    one runs through the line without a corner in it.
    """
    runs, run = [], 0.0
    for s in segs:
        if s["kind"] == "straight":
            run += s["length"]
        else:
            if run:
                runs.append(run)
            run = 0.0
    trailing = run
    # Through the line: whatever the lap ends on, plus the opening straight it rejoins.
    if trailing:
        runs.append(trailing + opening["length"])
    return max(runs) if runs else 0.0


def closes_to(segs, start):
    pose = start
    for s in segs:
        pose = advance(pose, s)
    return math.hypot(pose[0] - start[0], pose[1] - start[1])


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else random.randrange(1 << 30)
    rng = random.Random(seed)
    # Smaller ground on purpose.
    #
    # A 1700 m lap inside a 620 m plot is shorter than the plot's own perimeter — 2160 m
    # round the inside of the margin — so a ring fits comfortably and the walk has no reason
    # to fold inwards. It came out "a fucking giant ring" every time. Indiana is 525 m across
    # with a 2138 m lap: its lap is *longer* than its perimeter, so it has to double back
    # through its own middle, and that is what a track looks like.
    plot = 470.0
    # Ridden and called "little skinny": ten to thirteen and a half metres is the bottom of
    # what the corpus allows (8 to 20), and a national is wider than that.
    width = round(rng.uniform(14.5, 18.0), 1)
    grown = None
    for _ in range(6):
        # And longer: 1451 m rode as "overall small". Indiana is 2138.
        # Pulled back: 2400 m rode 'a bit too big'.
        grown = grow(rng, plot, width, rng.uniform(1650.0, 2050.0))
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
    # Importing it runs its own `main`, which writes a program of its own to stdout — and
    # two programs on one stream is not JSON. Swallow whatever it says.
    import io, contextlib
    saved, sys.argv = sys.argv, ["layout"]
    try:
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
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
            # Gently rolling, and no more. A lap is benched into whatever it crosses, so
            # ground with twenty metres of landform in it puts the track in a trench with
            # the banners along the rim of the cut — "we are back to be inside the ground".
            # A motocross venue is a field with shape in it, not a hillside.
            "relief": {
                "amplitude": round(rng.uniform(4.2, 7.5), 1),
                "wavelength": rng.choice([320, 380, 420, 480]),
                "seed": seed % 9973, "texture": 0.085,
                "tilt": round(rng.uniform(6.0, 16.0), 1),
                "tiltAngle": round(rng.uniform(0.0, 359.0), 1),
                "landforms": rng.randint(2, 4),
                "landformHeight": round(rng.uniform(2.0, 5.0), 1),
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
