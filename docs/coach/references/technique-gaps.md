# What five community sources agree on, and what Coach can see

Synthesis across the five videos in [community-tips.md](community-tips.md) and
[lynds-slow-to-pro.md](lynds-slow-to-pro.md), checked against
[COACHING.md](../COACHING.md) and the code on `main` (6ddc785d). Written 2026-09-17.
It supersedes the ranked list at the end of [lynds-vs-coach.md](lynds-vs-coach.md).

Sources: Lynds (cornering lesson; slow-to-pro; noob-to-pro), Aiden (top 10), WALK3R (beginner).
Three of the five are from riders with pro-series results, so where they agree it is worth
weighting; where a single video claims something, it stays a claim.

## The headline: the thing they all coach is the thing we cannot see

Four of the five sources build their core advice on **rider body position** — lean forward or
back, counter-lean or stay central. Lynds' entire cornering lesson is organised around it, and
his noob-to-pro video names the shift from the old meta (sit, lean back, counter-lean
everywhere) to the current one (stand, lean forward, body central) as the single biggest change
in the game.

Rider lean is not in the plugin API. COACHING.md has recorded this as the longest-standing
limitation and it is still true: we see the bike's lean, never the rider's. So the most
consistently coached skill in the community is invisible to Coach, and no amount of rule work
changes that without a memory read.

What we *can* see is stance. The recorder polls the rider's Sit control and writes STANCE, and
`stance_sit` / `stance_stand` compare against the reference lap. That is the right design, and
it quietly tracks the meta for free: because the target is a fast lap rather than a fixed rule,
a reference that stands through rough ruts will pull the rider up without anyone updating a
threshold. Worth noting the level gating already agrees with Lynds — `SIT` is excluded at
sub-pro and pro, and he says at that level you barely need a sit button.

Two things follow:

1. **Don't write rules that assume rider lean.** Several tempting ones ("lean forward here")
   cannot be verified and would be guesses dressed as coaching.
2. **Stance is the proxy we have.** It is currently one finding pair against the reference. Per
   corner type (below) it could be a good deal sharper.

## Braking bumps: four sources, one gap

This is the strongest corroboration in the whole corpus, and it lands on a gap we had already
found from one source:

- Lynds (slow-to-pro): brake and throttle together keeps the rear planted.
- Aiden (tip 1): lean back and use throttle to keep the bike level — his headline tip, and he
  goes out of his way to say it is not setup-dependent.
- Aiden (tip 2): use a **higher gear**, because first gear's engine braking is too strong and
  the bike sits revving with the rear coming up.
- WALK3R: the brakes barely work over braking bumps — engine brake instead.

Four independent riders describing the same problem and reaching for the throttle to solve it.
Coach has eight braking findings and none of them look at throttle and brake together;
`coasting` measures the opposite case. Both channels are already on the 1 m grid.

The gear half is new since the last review and is cheap: we already compare gear at the apex
(`gear_up` / `gear_down`). The same comparison across the braking zone, where the reference
carries a taller gear through the bumps, is the same machinery pointed 70 m earlier.

**Both map onto the existing `BRAKE` cue kind.** No plugin change, no new voice clip.

Still worth checking against the reference laps we hold before writing either — if our fastest
laps don't show the overlap, the rule doesn't get written.

## Corner types: our single undifferentiated `Corner`

Lynds' lesson is the most structurally useful thing in the corpus. He does not give tips, he
gives six corner types, and the correct technique differs per type — in places it inverts.
Counter-lean is right on a flat corner and wrong in every rut. Sitting is right on a smooth rut
and wrong on a hooked one.

Coach has `Kind { Straight, Corner, Jump, Rhythm, Whoops }`. Every corner on every track is the
same kind of thing, so every corner gets judged by the same rules.

We already hold most of what a classifier would need: curvature and turn angle, the rear wheel's
surface material per point and a main ground per section, suspension travel and acceleration
variance (smooth versus rough), bike roll, and whether the lap is on an SX layout. A first cut
separating flat / rutted / rough-rutted / sand from each other looks tractable, and it would let
existing findings be gated per type rather than fired everywhere.

This is the most valuable structural idea in the corpus, and also the largest. It is a change to
`features()` and `sections()` that every corner rule then reads, not a new rule.

## Rut entry: measurable, unmodelled

Lynds is specific and repeats it in every rut section: **join the rut at its start, with the
bike already near the rut's lean angle**. Coming in at too sharp an angle a third of the way
through compresses the suspension, and the rebound sends you over the rut.

Our `line` finding measures lateral offset at a **single point, the apex** (`analysis.rs:1162`).
Where along the corner the rider arrived at that line is not measured at all. The data is there:
offset across the whole entry rather than at one index, and roll rate at turn-in — though note
`roll_rate` is one of the channels decoded in `Sample` and never carried into `Point`, so it
would need plumbing.

This is a real finding shaped like a cue we already have (`INSIDE` / `WIDE`), from a pro-series
rider, on a mechanism he explains rather than asserts.

## Hopping and doubling the bumps

Lynds (noob-to-pro): anything you can double, do; go wide, miss most of the braking bumps,
double the last couple into the corner and land on the power. He frames it as more *reliable*
than ploughing through, not just faster.

This is the same behaviour as the line advice in the earlier video, seen from the jump side. Our
`jump_it` finding fires where the reference jumps and the lap rolls, which covers part of it. A
braking-zone version — the reference doubles into the corner and the rider ploughs — does not
exist, and would need the corner-entry bump content that a corner-type classifier would produce
anyway. Worth parking behind that work rather than before it.

## Consistency: three sources, and a training protocol

Lynds' five-steady-laps arithmetic, WALK3R's "just slow down", and Aiden's concrete protocol —
a 10–15 minute timer where the only goal is not crashing, and one-lap speed practised separately
afterwards. All three put consistency ahead of pace, and Aiden puts a method on it.

COACHING.md's "Not yet" already names this, and `Ideal` / `SectionBest.spread` is computed and
unused. The corpus adds the framing (total time over N laps, not best lap) and the suggestion
that Coach could recognise the two modes a rider is in — a no-crash session and a hot-lap
session are different activities and deserve different reviews.

## Front wheel light

Lynds devotes a section to being comfortable with the front wheel up: it is drag on the ground,
manualing rollers beats hitting the front on each, and some SX rhythms need the front picked up.
His sand-turn technique is manualing every bump.

We are less blind here than I assumed: `off[0]` already drives wheelie detection on corner exits
and stoppie detection under braking (`analysis.rs:1245`, `:1281`, `:1290`, `:1920`). What is not
modelled is the *deliberate* case — front light over rollers and through sand turns as a
technique rather than an exit error. Any rule here needs care, because the existing findings
treat a wheelie as something to fix.

## Setup churn, which argues with us

Aiden's seventh tip: don't keep changing your setup or your bike, because muscle memory needs
things to stay still.

Coach's setup engine writes an ordered list of changes into a copy of the rider's setup. The
ordering is already the right instinct. But a rider who runs a review after every session and
applies whatever comes out is doing exactly what Aiden warns against, and Coach currently has no
notion of how recently the setup moved or whether the last change was given a chance. The cue
side has `History` for exactly this reason — a call that has been on three sheets rests for two.
The setup side has no equivalent.

Cheap version: don't re-recommend a setting the rider changed in the last session or two.

## Outside the loop

Several things the corpus rates highly are not telemetry problems, and should not be forced into
Coach: controller settings (direct lean, gain, linearity — three top riders quoted at 40, 80 and
100), bike choice, suspension brand, sound mods as an audio gearing cue, look-back binds and
racecraft, and time and talent.

One of them is an *app* idea rather than a Coach idea: Lynds makes a concrete case that lower
graphics settings are a competitive advantage — low-quality track versions and shadows off make
bumps, ruts and jump sweet spots far easier to read. Two sources also push riding harder tracks
over farming the stock ones. Both sit closer to the manager than to Coach.

## Ranked

1. **Corner types.** The largest and the most valuable: it makes every existing corner rule
   better rather than adding one. Start with a classifier over data we already hold.
2. **Braking bumps: throttle-with-brake, and taller gear through the zone.** Four sources, both
   on the existing `BRAKE` cue, no plugin change. Validate against our reference laps first.
3. **Wire the line findings that already exist into cues** — `jump_line` and `height` arms in
   `from_finding()`, `WIDE`/`INSIDE` arms in `answers()`. Unchanged from the last review, still
   the cheapest real win.
4. **Rut entry** — offset across the entry rather than at the apex alone.
5. **Consistency over a session**, on total time.
6. **Setup churn** — a `History` equivalent for setup changes.
7. **Starts** — unchanged: blocked at three layers, and the input that matters most is one we
   cannot read.

1 to 6 are Coach-side. 7 crosses into FrostMod.
