# MXB Coach — how a lap is reviewed

MXB Coach compares a lap with a faster **reference lap** on the same track (by default the
fastest whole lap on that track, same bike first) and says where the time went and why. A few of
the things it says are judgements of the lap on its own terms, which hold whatever the clock says.

## Pipeline

1. **Record.** `mxbcoach.dlo` (frostmod repo, `src/mxbcoach.cpp` + `src/coachrec.h`) sits in
   `<MX Bikes>\plugins\` and writes `<user folder>\mxbcoach\sessions\*.mxbc` at 50 Hz: the
   game's `SPluginsBikeData_t` byte for byte, plus lap position, lap/split times and the
   centreline.
2. **Read.** `apps/coach/src-tauri/src/telemetry.rs` decodes the records and cuts laps at the
   line. A lap is *whole* when it starts and ends at the line, matches its reported time and
   has no crash; only whole, valid laps are compared.
3. **Align.** `analysis.rs` resamples both laps onto a 1 m grid along the centreline (lap
   position × track length), so index `i` is the same place on track in both.
4. **Split into sections** from the reference: corners (curvature tighter than 45 m, at least
   35° of turn, plus a 70 m braking zone and 25 m exit), jumps (both wheels off the ground
   ≥ 0.3 s and ≥ 6 m), rhythm (jumps within 30 m of each other) and whoops (three or more hops
   under 10 m long), and straights between.
5. **Name the sections** from the track's own frozen roster, so Turn 5 is the same corner next
   week (`trackmap.rs`, below).
6. **Classify corners for display** from the reference lap. Peak lean says flat versus held;
   mean vertical hit says smooth versus rough; rear-wheel material identifies sand; the height
   line against the corner's grade separates a rut from a berm; and a strongly rising path
   curvature identifies a hook. Clear combinations are shown as Lynds' six types: flat, smooth
   rut, hooked rut, rough rut, whooped sand turn, or smooth SX berm. Marginal readings remain
   unnamed, and these provisional display names do not change a coaching rule.
7. **Explain** each section that loses more than 0.05 s with the rules below. The three
   sections losing most are shown first. Safety findings (⚠: front lock, crooked or hard
   landing, sliding front) show even where no time was lost, and so do judgements the lap earns
   on its own terms (◆: over-jumping, casing, coming in too fast, turning in too early). A
   judgement is not a warning, though: it counts as an explanation, so it holds off the "Compare
   the traces" tip a section with nothing but warnings gets.

### Section names

A section's number is frozen per track. `trackmap.rs` keeps a roster of the track's
corners, jumps, rhythms and whoops in `<data dir>\coach\tracks\<track>.json`: the kind, a stable
id (`t5`, `j2`, `w1`), the name the rider sees, and the middle of the core in metres past the
line, averaged over the laps that have seen it. A grid index is already a stable track
coordinate, so what the roster adds is a stable *number*. The detected list moves: roll a jump
and the lap has one feature fewer, so everything after it counts up one, and a personal best
ridden on a slightly different line renumbers the whole track.

So the numbers are handed out once, from the union of the features over every lap the rider has
on the track, and never handed out again. A feature the map meets later takes the next free
number for its kind, which can leave it out of numeric order: a Turn 9 between 4 and 5. That is
the price of never renumbering, and it is worth paying, because renumbering silently
re-attributes the cue history and any per-corner progress. A section is matched to its landmark
by kind and then by the largest overlap of the cores, since a corner read forty metres longer on
one lap is still the same corner; with no overlap, by how close the two middles are (25 m). A map
built against a different centreline length, or by older code, is discarded and rebuilt.

The game's own centreline is not used for this, though the recorder does capture it. It is the
track builder's coarse driving line rather than the rutted berm people ride, its corner count
doesn't match what a rider feels, it says nothing about jumps or whoops, and its distances have
no origin in the recording: the start line's offset lives in race data the recorder never writes.

Straights keep their positional names. They are the leftovers between features, they move
whenever a feature's bounds move, and nobody says "meet me at straight four". `coach.rs`'s
`label` stamps the roster onto every list of sections the app builds (the review, the ideal lap's
section bests, and the lines view), so two screens can't call the same corner different things.

## Rules

Thresholds live in `analysis.rs` → `mod th`. ⚠ marks a safety finding; ◆ a judgement of the lap
on its own terms rather than a difference from the reference.

`tune_thresholds` (ignored; `COACH_LAPS=<dir of .mxbc> cargo test -p mxb-coach --bin mxb-coach
tune_thresholds -- --ignored --nocapture`) prints what the landing and turn-in rules read over
real recordings, and — the number that matters — how often each tip actually reaches the rider
per lap. Run it as recordings accumulate; a rule that fires more than about twice a lap is
crying wolf whatever its reasoning.

First pass, 3 recordings, 93 flights, 4 laps held against their own fast lap: landings run out
at a median gradient of −0.114 and descend over the lip at −0.322, landing hits sit at 3.6 G
median and 20.8 G at the 99th. `LAND_DOWN` moved from −0.12 to −0.07 on that evidence — the
first value left half of all real landings in the dead band. `in_too_hot` reached the rider 1.75
times a lap, in line with long-standing rules like `carry_speed` (3.0) and `coasting` (2.5), so
its gate was left alone; `apex_early` 0.25; the absolute over-jump did not fire at all, and only
2 of 93 flights earned a `Flat` verdict, so that verdict is if anything conservative. Four laps
is a small sample and none of it is settled.

| Section | Rule | Fires when |
|---|---|---|
| Corner | `brake_early` | brake onset > 5 m before the reference |
| Corner | `brake_late` | onset > 5 m after, and minimum speed < 95% |
| Corner | `brake_harder` | peak decel < 70% of the reference and zone > 120% as long |
| Corner | `brake_unneeded` | brakes where the reference doesn't |
| Corner | `more_front` | front share of braking < 50% where the reference's is ≥ 60% |
| Corner | `in_too_hot` ◆ | more than 30% of the turn-in speed still comes off after turn-in, at least 8 km/h of it, the brake lever held 0.25 s past turn-in, and a consequence: the slowest point past 60% of the core, the bike picked up 8° mid-corner, or 1.5 m wide on the way out (on its own: 40%, and no wide, which needs the reference line) |
| Corner | `carry_speed` | minimum speed < 95% of the reference |
| Corner | `apex_early` ◆ | slowest point in the first 35% of the core, and still leaned past 18° with the throttle shut at the end of it |
| Corner | `lean_more` | ≥ 5° less bike lean, and slower mid-corner |
| Corner | `line` | > 1.5 m off the reference line at its apex (tighter / wider) |
| Corner | `coasting` | 0.3 s more with no brake and no throttle |
| Corner | `late_throttle` | throttle held > 0.3 starts > 3 m later |
| Corner | `wheelspin` | rear slip > 1.15 for 0.3 s more on the exit |
| Corner | `gear_up` / `gear_down` | different gear at the apex and a slower exit |
| Corner | `exit_speed` | leaves > 3 km/h slower with no other exit cause |
| Corner | `clutch_braking` | clutch in > 0.5 s under braking, reference doesn't |
| Corner | `front_lock` ⚠ | front wheel < 80% of ground speed under front brake > 0.15 s |
| Corner | `bar_fight` | mean bar torque into the apex > 25 and > 1.5× the reference |
| Corner | `front_push` ⚠ | the bike turns < 75% of what its lean supports, the reference > 85% |
| Corner | second line | the slower of two lines within 0.25 s: named as the line for passing or for when the fast one ruts; a corner cutting up points to the other line ridden there |
| Corners | line pairs | two corners within 30 m of each other: laps grouped by their line in both; when the quickest pair over both isn't the one the first corner alone would pick (≥ 0.08 s), it's named and the first corner's own line note is dropped |
| Corner | `throttle_room` | ≥ 20 points less exit throttle than the reference, rear slip never over 1.08, front down (on its own: a ≥ 25 m exit under 55% throttle) |
| Jump | `jump_it` | reference jumps, lap rolls it |
| Jump | `chop_face` | throttle drops > 0.3 on the last 15 m of the face |
| Jump | `scrub` | > 10% + 0.1 s more airtime and > 0.5 m higher; technique is read against a scrubbed reference |
| Jump | `overjump` ◆ | the ground after touchdown runs flat past the downslope, and the bike dropped onto it and hit for it; said at the takeoff, the last place the rider can act |
| Jump | `land_short` ◆ | the ground after touchdown is still climbing: cased |
| Jump | `land_short` / `overjump` | where the ground can't settle it: lands > 2 m short of the reference / > 3 m past it |
| Jump | `land_throttle` | off the gas at touchdown, reference on it |
| Jump | `land_crooked` ⚠ | > 15° of lean 4 m after touchdown (a whip has unwound by then), 7.5° more than the reference |
| Jump | `land_hard` ⚠ | the landing hits > 10 G and > 1.4× the reference (on its own: > 12 G); not said where the landing already reads flat |
| Whoops | `whoops_speed` / `whoops_throttle` / `whoops_bucking` | slower, off the gas, pitching more |
| Straight | `shift_earlier` / `full_gas` | more time on the limiter / less throttle |

**A scrub is roll, not simply a bike that looks leaned in the air.** For each flight Coach
waits 0.3 s, integrates the recorded body-axis angular rates, and applies MXBMRP3's useful
classification boundary: yaw past 30° is a whip; otherwise takeoff lean or accumulated roll
past 30° is a scrub. That one classifier places the live cue and explains the review. Against a
scrubbed fast lap the review can now distinguish an upright attempt, a whip, and a roll started
after takeoff. Where the recorder knows the controls, it also compares sitting through the face
and forward rider input; unknown input stays unknown. A scrub that remains leaned after landing
gets a specific counter-lean warning, except on an up-face, where Lynds' deliberate cranked
landing is a recovery rather than a mistake.

The session debrief normally walks the three sections with the largest time loss. A section
with a `scrub` finding is appended when it is not already in those three, because the scrub is a
taught technique rather than a one-off warning. It uses the same section panel and six-type
corner label as the full review; no second analysis path is maintained for debrief.

**Where a jump was landed** is read off the ground rather than off the reference lap. The bike's
own height is the only terrain these rules have, so the landing zone is the ground the bike runs
on after touchdown: skip the first 2 m, where the suspension is still soaking the hit up, then
fit 8 m by least squares, which over that length is good to about a centimetre, an order below
the thresholds. Ground falling away at 0.12 m per metre or steeper is still the downslope, where
the landing is built to be taken. Flatter than 0.04 is past the bottom of it. Rising more than
0.06 is the up-face. The band between 0.04 and 0.12 is left without a verdict on purpose: a
shallow landing is the case the ground alone cannot settle, so the comparison with the reference
takes it back.

A flat run-out on its own is not over-jumping. A long low jump that settles onto flat ground is
fine, so the bike also has to have dropped onto it, coming down at 0.18 m per metre (about 10°),
and hit for it, at 0.7 of the rider's own hard-landing floor so `norm` carries straight over.
Flights under 0.4 s are hops off a bump and the ground either side of one says nothing. Where the
landing does read flat, `land_hard` is dropped: the G is the symptom, the over-jump is the cause,
and the over-jump tip quotes the figure anyway. Where a rhythm's jumps don't pair up against the
reference at all, a flat first landing is added to the count tip, because it is usually what
stopped the section linking.

Both verdicts run with a reference lap and alone, which is the point of reading the ground: a
jump over-jumped on every lap costs nothing against yourself and is still the thing worth saying.
What the bike's height cannot tell is a hill that keeps falling away, so a genuinely downhill
landing reads as a ramp: no judgement is made and the comparison with the reference takes it
back. That is the safe way round for it to be wrong. The app does read the track's own terrain to
draw the map, when the track is installed and readable, but these rules never see it, which is
also why they work on a locked track.

**Coming in too fast** is not a grip model. Nothing in the recording or the track files carries a
grip figure, and curvature measured from the rider's own path is a consequence of their entry
speed rather than a measure of the corner: come in hot, run a wider arc, and the radius grows to
fit the speed, so the error cancels. What is left is *when* the speed comes off. Braking belongs
before turn-in, so speed still leaving the bike after the bike is committed is speed that was
carried in. Turn-in is the first metre in the core past 18° of lean, not the core's start: the
corner detector's curvature gate is a 100 m arc, so a core opens long before a rider would say
they had turned in. A core shorter than 15 m has no inside of the turn to speak of and is left
alone.

The gate is the brake lever, not the deceleration. Sand and a deep rut scrub speed on their own,
and these rules cannot see the ground: a section's `soil` is filled in by the caller after the
review returns. The rule also wants a consequence before it says anything, or it fires on every
rider trail-braking into a rut, which in MX Bikes is how the corner is meant to be ridden.
`brake_late` is dropped in the same section: same cause, same fix, and saying both reads as the
tip repeated.

## Setup

Over the whole lap, with the fix for each in `fixes.rs` (the changes, in the order to try them,
written into a copy of the rider's setup where the bike's own option list for the setting is
known). Geometry is written too, with directions from the game: a later option raises the front
(fork height), lengthens the rod (lowering the rear about 3 mm per mm) and adds offset. The
swingarm's list is the bike's `.geom` (`rwheel_min`, `rwheel_max`, `swingarm_steps`), which runs
long to short on the 2003 Suzukis, so its direction is read per bike. A setting already past its
list is never written; swingarm pivot and rake never are.

| Tip | Fires when | Fix |
|---|---|---|
| `setup_bottoming_fork` / `_shock` | ≥ 3 bottom-outs a lap | fork: compression, oil, spring; shock: high-speed compression, spring, preload |
| `setup_bottoming_shock_slow` | the shock's bottom-outs mostly compress slower than 0.4 m/s | low-speed compression, spring, preload |
| `setup_stiff_fork` / `_shock` | never past 70% of the travel | softer compression, oil or spring |
| `setup_brake_dive` | fork past 85% braking into ≥ 2 corners | fork compression, oil, preload |
| `setup_exit_squat` | shock past 75% on the gas out of ≥ 2 corners | shock low-speed compression, preload |
| `setup_shock_kick` | rear extends faster than 0.6 m/s at the lip on ≥ 2 jumps | slower shock rebound |
| `setup_packing_fork` / `_shock` | in whoops, > 50% deep on average and never extending faster than 0.4 / 0.2 m/s | faster rebound, softer compression |
| `setup_rear_low` / `setup_front_low` | on steady straights the shock sits > 30% deeper than the fork / the fork deeper than the shock | preload, then a shorter rod / the fork down in the clamps |
| `setup_front_push` | the front slides in ≥ 2 corners | softer fork compression, more shock preload, then the fork up in the clamps |
| `setup_unstable` / `setup_turns_slow` | only from the feel check | front higher, less offset, longer swingarm / the other way |
| `setup_sag_rear_deep` / `_high` | standing still ≥ 1 s with the rider on, the shock outside 30–36% of its travel | shock preload by the millimetres it's off; the spring when preload runs out |
| `setup_pressure` | a tyre > 10 kPa from the `OptimalPressure` in its `.tyre` file | back to the tyre's optimum |
| `setup_sand` / `_hardpack` / `_mud` | ≥ 30% of the lap on sand / ≥ 50% on hardpack or wet soil (`soil.rs`) | firmer shock low-speed and fork compression, +1 rear tooth / softer compression / −1 rear tooth |
| `setup_gearing_*`, `setup_shift_*`, `setup_swingarm` | limiter, bogging, shift points, front up on exits | rear sprocket; a longer swingarm |

The ground comes from the rear wheel's material as the recorder gives it: the game's global
surface list plus one (0 in the air). 11 is soft soil, 12 compact soil (hardpack), 8 soil, 6
sand, 5 grass, 13–14 gravel and rock; there is no mud, so soil in rainy conditions counts as mud.
No track file is read, so locked tracks work. Each section carries its main ground, worked out
after the review, which is why no rule in the review can read it.

The suspension, acceleration and bar thresholds come from real laps (2026-09-15, five laps of a
250F): a landing's hit has a median of 5 G and a 90th percentile of 10 G, the bars into a
corner a median of 17. `analysis::norm` scales the hard-landing and bar floors by the rider's
own session against those (clamped 0.7–1.5, from 8 landings or 200 m of riding), so a heavy bike
or a light 85 is judged against itself.

**Feel check.** The rider picks what they feel from thirteen feels in the Setup card, each
mapped to a tip. A feel the review also found is confirmed; one the travel used argues with
(bottoming under 90% of travel, harsh at 95% or more, from `sag::travel_used`) says so and adds
no fix; the rest add their fix on the rider's word. The game's brake pressure channel carries no data, so braking is read
from the lever inputs. The OEM MX tyres don't heat or wear in the game (heating factors and wear
rate are 0), so pressure is judged against the tyre's optimum only. Sag is measured standing
still when the recording has a second of it; riding sag is shown but not held to a target.

## Live cues

`cues.rs` turns a review into the few calls the recorder shows during a lap in practice.
The fast lap says where each call goes (`analysis::cue_points`): the corner's braking point,
brake release, turn-in (sit), back on the gas, a scrub at a takeoff, standing into a rhythm or
whoops. Each candidate is rated by the time its section loses, doubled when one of the
section's tips is about the same thing, plus how basic the call is at the rider's level.

Two kinds of call come from the section's own tips rather than from the fast lap's inputs:

- **Line** — the `line` tip names a side, so the cue does: "Stay wide" or "Go inside here".
- **Shift** — only from `gear_up` and `gear_down`, which fire where the rider's gear leaves
  them slower out of the corner than the fast lap. Where the fast lap happens to change gear
  is **not** a reason to call a shift, and the cue names a gear to be in ("One gear higher"),
  never a moment to change ("shift earlier"), which a rider carrying speed reads as "go
  slower". Both were rider reports against the first sheet that shipped.

The judgements don't raise a call yet. There is no cue kind for over-jumping, and `in_too_hot`
and `land_short` aren't in the mapping that counts a tip as answered by a call, so a corner the
rider arrives at too hot only lifts its brake cue through the time the section loses.

Every cue is an instruction at a spot, under 26 characters — what the plugin's cue box fits —
and never a summary: "Take the fast line" is the example of what not to write. The plugin
picks the spoken clip by cue kind, not by this text, so the wording is free to change.

| Level | Calls | Section must lose |
|---|---|---|
| New | brake, gas, stand, sit | nothing: the basics everywhere |
| Intermediate | + off the brakes, shift up, shift down | 0.05 s |
| Sub-pro | + stay wide, go inside, scrub (no sit) | 0.1 s |
| Pro | the same | 0.15 s |

How much: a few (2 a lap, 5 s apart), normal (4, 3 s), lots (6, 2 s); cues closer than 30 m
keep the more important one. The file is `<user folder>\mxbcoach\cues\<track>.<bike>.cue`,
`MXCQ` version 1, read by FrostMod's `src/coachcue.h`; the plugin shows each cue 1.2 s before
its spot at the bike's speed for 1.5 s, only in testing or a race event's practice session.

### What Coach writes for the recorder

Two settings files under `<user folder>\mxbcoach`, both merged key by key so anything Coach
doesn't set stays as it was (`ini.rs`). Every part is on unless the file says otherwise, with
three exceptions.

`hud.ini`, `[hud]`: `enabled`, `cue`, `section`, `gap`, `stance`, `map`, `susp`, `trail`,
`setup`, plus `cue_x` and `cue_y`.

- **`map`** defaults *off* when `plugins\mxbmrp3.dlo` sits beside the recorder, because MXBMRP3
  draws its own. Coach reports that state rather than a plain "on", which is what made the
  switch look broken, and always writes the key out explicitly when the rider touches it.
- **`susp`** (suspension bars per end, with a bottomed mark) and **`trail`** (the reference lap
  as a blue trail ahead on the map) default *off*: they are information over the game's own
  screen rather than coaching.
- **`cue_x`** is the centre of the cue box across the screen and **`cue_y`** its top edge, both
  fractions with (0,0) top left; the default pair is the plugin's own box, `0.5` and `0.285`.
  The section line follows the box. Out of range or unreadable keeps the default.

`cues\voice.ini`, `[voice]`: `enabled`, `volume` (0–100) and `voice` (`female` or `male`;
anything else is `female`, as the plugin reads it).

`susp`, `trail`, `cue_x`/`cue_y` and `voice` need the recorder from FrostMod 0.24. Coach writes
them whatever version ran — an older recorder ignores what it doesn't know — and says in the
panel when the version that last ran is older than that.

### The sheet moves on

A rider told the same four things every lap stops hearing them, so a sheet is not written once
per track and bike and left there. Each sheet is picked against a `History` of what the last
ones said (`<data dir>\coach\cues\<track>.<bike>.json`): a call that has been on three sheets
in a row rests for the next two, scoring a fifth of its worth while it does, and whatever is
costing time now takes the place it leaves. A call the rider has actually taken needs no rule
— its section stops losing time, so it stops qualifying and is dropped from the history
altogether.

The history keys on the section's stable id, not its name, so a call rests at the corner the
rider actually heard it at. Version 1 keyed on the name, which a new personal best could
renumber, so a history of any other version than the current one (`cues::HISTORY_VERSION`, now
2) is dropped rather than migrated: translating a v1 file needs the very track map that did not
exist when it was written, and it would be wrong exactly on the tracks whose numbering had
already drifted. The cost is one sheet that doesn't know what the rider has heard, and the sheet
after it has rebuilt the record.

Rewriting rather than shipping several sheets at once is deliberate: the `.cue` layout is a
contract with FrostMod, the plugin has no way to choose between sets, and Coach already
watches the sessions folder, so it can pick again from the newest lap while the rider is still
out. `coach_write_cues` takes `latest`, which does exactly that.

## Other riders

Recorders from FrostMod 0.21 write ENTRY (tag 12, who's in the event), POSITIONS (tag 13, every
bike's track position and x/y/z about ten times a second, with the bike nearest the rider's
telemetry flagged) and RACE_LAP (tag 14, every rider's timed laps). The plugin API gives only the
rider's own GUID, never anyone else's, so the rider recording is the entry with the name EventInit
gave them, the nearest-bike flag deciding between two of the same name. `others.rs` cuts each
other rider's valid laps from their positions, as metres against time, and times this lap's own
sections on them. The review names up to three: the rider just faster than this lap, the fastest
in the session and, in a race, the rider ahead on track when the lap ended, each with the
sections where they gain more than 0.05 s (at most three).

`lines.rs` also places every other rider's position beside each corner's core against the fast
line (nearest point within 8 m, ends of the core excluded). With 30 or more, the median side is
where the crowd rides: within 1 m, the fast line will rut first; further out, the crowd's line
will and the fast line stays smoother (`wear` notes).

## Where MX Bikes differs from real life

- **Scrubbing** is done seated in MX Bikes: lean the bike leaving the lip, lean the other way
  in the air to straighten before landing (MX Bikes' own tip). Real-life coaching stands.
- **Rider lean is its own input.** Players counter-balance (rider slightly to the outside).
  The plugin API doesn't expose rider lean, so the coach can only see the bike's lean.
- **Aids blur the channels.** Combined brakes feed the rear from the front lever; auto clutch
  hides clutch technique. Brake-balance and clutch advice is worded with that in mind.
- **Ruts deepen through a session**, so a reference from a fresh track can be faster for
  reasons that aren't the rider.

## Not yet

- Rider lean (needs a memory read; not in the plugin API). Sitting and standing are read: the
  recorder polls the rider's own Sit control from `controls.txt` (FrostMod `src/stance.h`) and
  writes STANCE_BIND (tag 10) and STANCE (tag 11). `stance_sit` fires where the fast lap sits
  ≥ 30% more of a corner from turn-in, `stance_stand` where it stands ≥ 30% more of whoops or a
  rhythm; both need the stance known for 60% of the section on both laps.
- Riding-aid settings and deformation from `profile.ini`, stored with each session.
- Starts (launch, wheelie, bog) and consistency across a session beyond section spread.
- A live call for the new judgements. Over-jumping wants "Roll this one", which needs a cue kind
  here and a clip in the plugin (`ROLL = 12` is the plan; the kinds stop at 10 today), and
  `in_too_hot` and `land_short` have to count as answered by the brake and throttle calls before
  they can lift one.
- Grip. Nothing in the recording or the track files carries a figure for it, so no rule can say
  a speed was too fast for the surface; `in_too_hot` reads the consequence instead.
- The ground under the rules. A section's soil is worked out after the review returns and the
  terrain is only read to draw the map, so no rule can make allowances for a sand corner, a
  rutted berm or a downhill landing.

## Sources

Transmoto (braking, corner entry, jumps), Gary Semics (braking, clutch, scrubbing), Lynds
([slow-to-pro scrub lesson](references/lynds-slow-to-pro.md)), Shane Watts (tight turns), MXA
(momentum, shifting, ruts), MotoOnline / CRA (whoops), Cycle World (starts), MX Bikes on X
(scrub method), MX Bikes Steam threads and forum (lean, aids, gearbox preload, deformation),
PiBoSo's `mxb_example.c` (the API), and Thomas's MIT-licensed
[MXBMRP3 air-trick classifier](https://github.com/thomas4f/mxbmrp3/tree/a6678de9fe86d683516449ffebc3fe80b1383767)
(scrub/whip boundary and airtime gate).
