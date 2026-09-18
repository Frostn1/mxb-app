# Plan: telling corners apart

A design for review, not a change. Written 2026-09-17.

## Why

Coach has one corner. `Kind { Straight, Corner, Jump, Rhythm, Whoops }` — every corner on
every track is the same kind of thing, so every corner is judged by the same rules.

The corpus in [references/technique-gaps.md](references/technique-gaps.md) says that is wrong in
a way that matters. Lynds' cornering lesson gives six corner types whose correct technique
differs and in places **inverts**: counter-lean is right on a flat corner and wrong in every
rut; sitting is right on a smooth rut and wrong on a hooked one; standing is mandatory in a
whooped sand turn and optional on a smooth one. A coach that cannot tell a flat corner from a
rut is, on those points, as likely to be wrong as right.

## The main risk, and the shape that avoids it

Every corner rule reads `sections()`. A classifier that splits corners badly does not fail
loudly — it quietly makes existing findings worse on the corners it mislabels, and we have no
real laps on this machine to notice with.

So the design is **additive and gated**, in this order:

1. Add the classification as new fields on `Section`. Nothing reads them.
2. Verify the classifier on real laps, by eye, against tracks we know.
3. Only then gate individual rules on it, one at a time, each with its own test.

At no point does an existing rule change behaviour because the classifier exists. If step 2
never happens, Coach behaves exactly as it does today.

## Don't classify into six types. Classify four properties.

The six named types are not independent — they are combinations. A "rough rut" is rutted plus
rough; a "whooped sand turn" is sand plus rough plus rutted. Naming six and picking one forces a
single winner where a corner is often two things, and it gives us nothing to fall back on when
the call is marginal.

Four independent properties instead, each with an explicit `Unknown`:

| Property | Values | Read from |
|---|---|---|
| `rutted` | Flat / Rutted / Unknown | peak bike roll held through the core, and the ground height at the apex against the corner's entry and exit (a rut sits below, a berm rises) |
| `rough` | Smooth / Rough / Unknown | suspension velocity and vertical hit through the core — `sv_hi` / `sv_lo` and `Point.hit`, which already carry the per-metre extremes |
| `surface` | Sand / Soft / Hard / Unknown | `Point.ground`, the rear wheel's material, already summarised per section and already mapped by `soil.rs` |
| `shape` | Constant / Hooked / Unknown | the curvature profile across the core: a hooked rut tightens towards the exit, which is a rising \|k\| in the last third against the first |

The six names then fall out for display only — a corner shown as "Turn 4, rough rut" is just the
two properties written out. Rules gate on the properties, not the names.

Every property defaults to `Unknown`, and **`Unknown` must mean "behave as today"** in every
rule that gates on it. That is the property that keeps step 3 safe.

## Where it goes

In `features()`, after the curvature cores are built and the air features have been folded in,
so a corner is already its final extent. The classifier reads the reference `Trace` over the
corner's core and returns the four properties; `Section` carries them through `sections()`
unchanged.

It runs on the **reference lap only**. The corner is a property of the track, not of the lap
being reviewed, and both laps are on the same 1 m grid at the same place — so classifying the
reference keeps a corner's type stable across every review of that track.

## What we can tune here, and what we can't

We can write the classifier and its tests against the synthetic laps in `analysis::tests`, which
already have knobs for the things that matter: `sink` (ground cut lower, a rut), `wide`,
`corner_v`, `bottom` and `hit` (suspension and landing force), and `torque`. That is enough to
prove the classifier responds to the right inputs in the right direction.

It is **not** enough to set thresholds. Those need real laps on known tracks — a flat Forest
corner, a rutted pro-track corner, a sand turn — and there are no `.mxbc` recordings on this
machine. So the honest plan is: land the classifier with thresholds marked provisional in
`mod th`, exactly as the existing suspension and bar thresholds were before real laps tuned
them, and treat step 2 as a separate piece of work that needs recordings.

## Which rules would gate, once it's verified

Not part of this change; listed so the shape is visible.

| Rule | Gate |
|---|---|
| `stance_sit` / `stance_stand` | the strongest one. Standing is right in rough and hooked ruts, optional in smooth ones, and sitting is right on flat corners. Today one rule covers all of them |
| `lean_more` | a rut holds the bike; lean is far less of a choice there than on a flat corner |
| `line` | a rut gives the rider much less lateral choice, so the 1.5 m threshold means something different |
| `front_push` / `setup_front_push` | a front slide on a flat corner and a front pushed out of a rut are different faults |
| `carry_speed` | what "slow" means differs between a sand turn and a smooth berm |
| a rut-entry rule | only meaningful where there is a rut to enter — this is the item the corpus raised, and it needs `rutted` before it can exist at all |

## What the recordings showed (2026-09-17)

Three recordings arrived: one on Forest Raceway with **no comparable laps at all**, and two on
755 Compound (KTM 450) with two and four. So the calibration below rests on **one track, one
bike and one surface** — every corner in all of it is soft soil. Enough to check the machinery
responds to the right things and agrees with itself; nowhere near enough to call the thresholds
settled.

What it settled:

- **Peak lean separates corners cleanly.** One corner read 40–44° in both sessions while every
  other read 55–81°. That gap is where `RUT_LEAN_DEG` and `FLAT_LEAN_DEG` now sit.
- **Peak hit is useless, mean hit is usable.** The peak moved by more than 2 G between two
  sessions on the same corner — it is one sample. The mean moved by about 0.3.
- **Mean lean is useless.** It moved by up to 20° on the same corner between sessions.
- **The hook metric is noise.** First-third against last-third turn rate gave ratios from 0.17
  to 4.7 with no agreement between sessions. `shape` is therefore not implemented, and should
  not be until there is a better measurement than that.

Cross-session agreement over the same eleven corners, which is the test that matters:

| Property | Agreed | Abstained on one side | **Contradicted** |
|---|---|---|---|
| `hold` | 9 / 11 | 2 | **0** |
| `bumps` | 5 / 11 | 6 | **0** |

Zero contradictions is the property worth having: no corner ever flipped Flat to Rutted or
Smooth to Rough. `bumps` abstains more than it decides, which is the right way to fail, but it
is weak and should gate nothing until better recordings say more.

Not calibrated at all: **`soil`**, because there is one surface in this data. The mapping is
reused from `soil.rs` so it is mechanically right, but nothing here shows it separating a sand
turn from hardpack in practice.

### One thing this turned up that matters more than the classifier

**Corner extents are not stable between sessions on the same track.** The same corner came back
as 166 m in one session and 73 m in the other; another as 50 m and 31 m. `features()` derives
corners from whichever lap is the reference, so two reviews of the same track are not measuring
the same piece of ground.

The classifier is built to survive that — the properties that held up are the ones least
sensitive to where a corner is cut — but it undermines the assumption written above that
classifying the reference "keeps a corner's type stable across every review of that track".
Stabilising corner extents is now the more valuable piece of work, and a prerequisite for
anything that compares a corner with itself over time.

## Steps

1. `CornerType` (the four properties) on `Section`, classifier in `features()`, provisional
   thresholds, tests against the synthetic laps for direction of response. Nothing reads it.
2. Show the classification in the review UI so it can be eyeballed against real tracks. Cheap,
   and it is how step 3 gets verified.
3. Recordings on known tracks; tune the thresholds.
4. Gate rules one at a time, each with its own test, `Unknown` always meaning today's behaviour.

Steps 1 and 2 are safe to do now. Step 3 needs laps. Step 4 needs step 3.

## What this does not do

It does not give us rider lean — that is [rider-lean-plan.md](rider-lean-plan.md) — and several
of Lynds' per-type points are about rider body position, so they stay out of reach until that
lands. Corner types make the stance rules sharper, which is the part we can see.
