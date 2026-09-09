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

What it lays is measured rather than chosen. A corner is drawn whole and the ground between
corners is drawn from the corpus's own radius distribution — see the two tables below, and
`scripts/track-survey.py`, which reads them off the published tracks' own centrelines. And the
moves live on a stack, because a walk that may not touch itself is a self-avoiding walk and a
greedy one paints itself in.

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
        """Mark the ground, and hand back what to unmark if the move is taken back."""
        marked = []
        for x, z, _ in pts:
            i = self.index(x, z)
            if i is not None and not self.seen[i]:
                self.seen[i] = True
                marked.append(i)
        return marked

    def undo(self, marked):
        for i in marked:
            self.seen[i] = False


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
        """Lay ground, and hand back how much each cell grew by so it can be taken back."""
        grew = {}
        for x, z, along in pts:
            i = int(z / self.cell) * self.n + int(x / self.cell)
            if 0 <= i < len(self.used):
                self.used[i].append((x, z, at + along))
                grew[i] = grew.get(i, 0) + 1
        return grew

    def undo(self, grew):
        for i, k in grew.items():
            del self.used[i][-k:]

    def blocked(self, x, z, ignore_after, clear, ignore_before=-1.0):
        """Is anything already laid within `clear` of here? Stops at the first one.

        `nearest` measures, which costs a sweep of every cell inside sixty metres — 225 of
        them — when the walk only ever asks whether one point is too close. This looks at 25.

        Both bounds are ages round the lap: the last thirty metres, because a corner may come
        close to the run that fed it, and the opening straight, because the way home runs up
        beside it.
        """
        for i in self._cells(x, z, clear):
            for px, pz, age in self.used[i]:
                if age > ignore_after or age < ignore_before:
                    continue
                if (px - x) ** 2 + (pz - z) ** 2 < clear * clear:
                    return True
        return False

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


# What a lap is made of. `scripts/track-survey.py` prints this off the tracks' own `.trh`
# centrelines; a corner is same-handed turning under a 300 m radius with under ten metres let
# into it.
#
#                     Indiana   Southwick   all 18
#   lap                 2170 m     2217 m   1065-3055
#   corners           16 (7.4)   18 (8.1)   7.4-10.3 per km
#   arcs in a corner         4          5   1-6
#   a corner's angle       159d       166d   90-166 (p50 136)
#   apex radius         10.4 m     11.9 m   7.1-18.5
#   ground per corner     68 m       74 m   14-82
#   run between them      27 m       30 m   20-72
CORNERS_PER_KM = (7.4, 10.3)
LAP_MAX_M = 2550.0          # `trackllm::corpus::LAP_M` refuses anything over 2600
TIGHT_SHARE = 0.10          # how much of a lap is tight enough to wear a rut

# And the ground between the corners, as length by radius. A published lap is almost never
# straight and a third of it drifts through a radius over 300 m, which rides as a straight and
# measures as an arc. A wander of 55-190 m instead read as part of the corner beside it.
#
#                    Indiana   Southwick
#   a true straight     2.9%        0.8%
#   R 40-80 m          16.7%       14.7%
#   R 80-160           14.1%       13.6%
#   R 160-300          11.2%       13.4%
#   R 300-1000         18.1%       19.0%
#   R over 1000        15.9%        8.2%
WANDER = [(40.0, 80.0, 0.21), (80.0, 160.0, 0.19), (160.0, 300.0, 0.17),
          (300.0, 1000.0, 0.26), (1000.0, 3000.0, 0.17)]


def a_wander(rng):
    """A piece of the ground between corners: a radius off the corpus, and 15 to 45 m of it.

    Drawn as a length with the angle following, because a fixed angle band makes a 500 m
    sweeper 200 m long. Indiana's segments average eighteen metres.
    """
    roll, acc = rng.random(), 0.0
    lo, hi = WANDER[-1][0], WANDER[-1][1]
    for a, b, w in WANDER:
        acc += w
        if roll <= acc:
            lo, hi = a, b
            break
    r = math.exp(rng.uniform(math.log(lo), math.log(hi)))
    run = rng.uniform(15.0, 45.0)
    side = rng.choice((1.0, -1.0))
    return [{"kind": "arc", "radius": r * side,
             "angle": math.degrees(run / r), "rise": 0.0}]


def a_corner(rng, side=None):
    """One corner, built the way a real one is: in on a loosening arc, round, and out.

    A published corner is not one arc — Indiana's median is four and Southwick's five — which
    is why it covers seventy metres of ground turning through 160 degrees where a lone arc of
    the same apex radius covers thirty. Offered one arc at a time the walk could only reach
    the corpus's tight radii with a hairpin, and a same-handed wander landing beside it read
    as one corner of 197 degrees.
    """
    if side is None:
        side = rng.choice((1.0, -1.0))
    shape = rng.random()
    if shape < 0.22:
        # Not every corner is a hairpin: a fifth of the corpus's are a single sweep.
        return [{"kind": "arc", "radius": side * rng.uniform(15.0, 32.0),
                 "angle": rng.uniform(40.0, 95.0), "rise": 0.0}]
    entry = {"kind": "arc", "radius": side * rng.uniform(26.0, 60.0),
             "angle": rng.uniform(22.0, 50.0), "rise": 0.0}
    apex = {"kind": "arc", "radius": side * rng.uniform(8.0, 15.0),
            "angle": rng.uniform(55.0, 105.0), "rise": 0.0}
    exit_ = {"kind": "arc", "radius": side * rng.uniform(18.0, 45.0),
             "angle": rng.uniform(18.0, 45.0), "rise": 0.0}
    return [entry, apex, exit_]


def chain_length(segs):
    return sum(s["length"] if s["kind"] == "straight"
               else abs(s["radius"]) * math.radians(s["angle"]) for s in segs)


def grow(rng, plot, width, want_m):
    """Walk a lap out of the ground and dock it back onto its own start.

    Each step offers the same handful of moves — a run, a gentle wander, or a whole corner
    either way — and the one taken is whichever leaves the track in the most open ground while
    clearing everything already laid. When the budget runs down, the shortest Dubins path back
    to the start pose that is also clear closes the lap exactly, which is why there is no
    return leg to route round anything.

    And it takes moves back. Greedy, sixty seeds gave one lap and the other fifty-nine ran out
    of legal moves 150 to 900 m in, because the first dead end was final. The moves live on a
    stack now, and a dead end pops one and takes the next-best instead.
    """
    clear = width + 5.0          # how near the lap may come to itself
    margin = 26.0
    # Drawn once: redrawn at every step it averages to the middle and the noise decides.
    want_rate = rng.uniform(*CORNERS_PER_KM) / 1000.0
    gate_room = 78.0        # the start spur stands beside the lap and needs ground
    start = (plot * 0.5, margin + gate_room, 0.0)   # facing +z, up the plot
    ground = Ground(plot)
    cover = Cover(plot)
    segs = []
    # The first stretch is the start straight, and nothing may be built on it.
    opening = {"kind": "straight", "length": rng.uniform(90.0, 130.0), "rise": 0.0}
    segs.append(opening)
    ground.add(samples(start, opening), 0.0)
    cover.add(samples(start, opening))
    pose = advance(start, opening)
    laid = opening["length"]

    def legal(from_pose, chain, age_cut, skip_before=-1.0):
        p = from_pose
        for seg in chain:
            for x, z, _ in samples(p, seg, 2.5):
                if not (margin <= x <= plot - margin and margin <= z <= plot - margin):
                    return False
                if ground.blocked(x, z, age_cut, clear, skip_before):
                    return False
            p = advance(p, seg)
        return True

    def offers(pose, laid, since_corner, corners_laid, tight_m):
        """Every move worth trying here, best first.

        Scored on ground it would be the first to visit — per metre travelled, not per move,
        because rewarding raw coverage buys it with long straights, and a lap of long runs
        with angles between them is not a track.
        """
        running = 0.0
        for prev in reversed(segs):
            if prev["kind"] != "straight":
                break
            running += prev["length"]
        cand = []
        # A straight, unless the lap is already on one long enough. Consecutive straights are
        # colinear, so the app's `straight_runs` reads a row of them as ONE straight — and a
        # row of 35-80 m moves reached 248 m against the 125 m cap (`corpus::STRAIGHT_M`, the
        # FFM's limit and the only one any federation writes).
        room_on_the_straight = 125.0 - running
        if room_on_the_straight > 30.0:
            cand.append([{"kind": "straight",
                          "length": rng.uniform(30.0, min(75.0, room_on_the_straight)),
                          "rise": 0.0}])
        # The lap's own wander, and most of the ground between corners.
        for _ in range(7):
            cand.append(a_wander(rng))
        # And corners, whole — but only once the lap has run far enough since the last one for
        # the two to be told apart, or a same-handed pair reads as one corner of twice the
        # angle. The corpus's p50 run between corners is 27 to 30 m.
        if since_corner >= 20.0:
            for _ in range(4):
                cand.append(a_corner(rng))

        behind = corners_laid < laid * want_rate
        scored = []
        for chain in cand:
            run_m = chain_length(chain)
            pts = []
            p = pose
            for seg in chain:
                pts += samples(p, seg, 6.0)
                p = advance(p, seg)
            score = 190.0 * cover.fresh(pts) / max(run_m, 1.0)
            ahead = (p[0] + math.sin(p[2]) * 26.0, p[1] + math.cos(p[2]) * 26.0)
            score += 0.35 * ground.room(ahead[0], ahead[1], laid - 34.0)
            corner = len(chain) > 1 or (chain[0]["kind"] == "arc"
                                        and chain[0]["angle"] >= 40.0)
            if corner:
                # Corners are the point — but eight a kilometre, not as many as will fit.
                score += 34.0 if behind else -22.0
                apex = min(abs(s["radius"]) for s in chain)
                if apex < 14.0 and tight_m < laid * TIGHT_SHARE:
                    score += 20.0
            scored.append((score, chain))
        scored.sort(key=lambda t: -t[0])
        return [c for _, c in scored]

    def home_from(pose, laid, segs):
        """A way back onto the start pose that lands, is clear, and breaks no rule."""
        # Several radii, not one. A Dubins path is fixed by the radius you give it, so a
        # single draw is a single shape, and two seeds in three walked on to the cap instead
        # of closing.
        ways = []
        for r in (14.0, 18.0, 23.0, 29.0, 36.0):
            ways += dubins(pose, start, r * rng.uniform(0.92, 1.08))
        ways.sort(key=lambda t: t[0])
        for _, home in ways:
            # Walked, not trusted: one of the four families has a sign in it that only bites
            # on some geometries, and the symptom is a lap that misses itself by thirty metres.
            landed = pose
            for seg in home:
                landed = advance(landed, seg)
            if math.hypot(landed[0] - start[0], landed[1] - start[1]) > 0.25:
                continue
            # The way home may run up beside the start straight — that is where it is going —
            # so the opening straight is not an obstacle to it, and nothing else is excused.
            #
            # This used to be a disc round the start pose, which excused every *other* piece
            # of lap that happened to pass through the disc too: the app's `review` caught a
            # way home running 12 m from a mid-lap straight on a 15 m track. Ages are exact
            # where a radius is not.
            if not legal(pose, home, laid - 34.0, skip_before=opening["length"]):
                continue
            # And no straight on the finished lap may run past the FFM's 125 m. Checked on the
            # whole lap: consecutive straights are colinear so `straight_runs` reads a row of
            # them as one, and a Dubins path's straight sits in its middle.
            if longest_straight(segs + home, opening) > 125.0:
                continue
            # The way home is part of the lap and its own length counts: laps came out at
            # 2627 and 2820 m because the walk stopped at the cap and Dubins added to it.
            if laid + chain_length(home) > LAP_MAX_M:
                continue
            if all(s["kind"] != "arc" or s["angle"] < 200.0 for s in home):
                return home
        return None

    cap = min(want_m * 1.25, LAP_MAX_M)
    # Each entry is one move on the lap and what it would take to undo it.
    stack = []
    budget = 9000           # moves tried, so a hopeless seed gives up rather than hangs
    since_corner, corners_laid, tight_m = 0.0, 0, 0.0
    node = None
    while budget > 0:
        if node is None:
            if laid > want_m * 0.86:
                home = home_from(pose, laid, segs)
                if home:
                    return segs + home, start
            node = {"cands": (offers(pose, laid, since_corner, corners_laid, tight_m)
                              if laid < cap else []),
                    "i": 0}
        budget -= 1
        # Take the best move left here that is actually clear.
        chain = None
        while node["i"] < len(node["cands"]):
            c = node["cands"][node["i"]]
            node["i"] += 1
            # Ignore the last thirty metres of track when checking clearance: a corner is
            # allowed to come close to the run that fed it.
            if legal(pose, c, laid - 34.0):
                chain = c
                break
        if chain is None:
            # Painted in. Take the last move back and try this node's next-best instead.
            if not stack:
                return None
            back = stack.pop()
            ground.undo(back["ground"])
            cover.undo(back["cover"])
            del segs[len(segs) - back["n"]:]
            pose, laid = back["pose"], back["laid"]
            since_corner, corners_laid = back["since"], back["corners"]
            tight_m = back["tight"]
            node = back["node"]
            continue
        was = dict(pose=pose, laid=laid, n=len(chain), node=node,
                   since=since_corner, corners=corners_laid, tight=tight_m)
        g, cv = {}, []
        p, at = pose, laid
        for seg in chain:
            pts = samples(p, seg)
            for i, k in ground.add(pts, at).items():
                g[i] = g.get(i, 0) + k
            cv += cover.add(pts)
            at += chain_length([seg])
            p = advance(p, seg)
        was["ground"], was["cover"] = g, cv
        stack.append(was)
        run = chain_length(chain)
        corner = len(chain) > 1 or (chain[0]["kind"] == "arc" and chain[0]["angle"] >= 40.0)
        if corner:
            corners_laid += 1
            since_corner = 0.0
            tight_m += sum(abs(s["radius"]) * math.radians(s["angle"]) for s in chain
                           if abs(s["radius"]) < 14.0)
        else:
            since_corner += run
        pose, laid = p, laid + run
        segs += chain
        node = None
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
        grown = grow(rng, plot, width, rng.uniform(2000.0, 2350.0))
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
