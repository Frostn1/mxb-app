# Lynds' five techniques vs MXB Coach

How the techniques in [lynds-slow-to-pro.md](lynds-slow-to-pro.md) line up with what
[COACHING.md](../COACHING.md) describes and with what the code actually does. Written
2026-09-17 against `origin/main` (de3069ab).

> The ranked list at the end of this file is superseded by
> [technique-gaps.md](technique-gaps.md), which weighs this video against four more
> sources. The per-technique comparison below still stands.

One thing to hold on to while reading: a finding only becomes a spoken cue if it has an arm in
`cues::answers()` (`cues.rs:144-155`) or `cues::from_finding()` (`cues.rs:161-175`). Plenty of
analysis exists that never reaches the rider in the bike.

## 1. Table-topping jumps — covered in principle, not in placement

COACHING.md already has the technique and has it right: scrubbing in MX Bikes is done seated,
leaning the bike off the lip and back the other way in the air. `SCRUB` (kind 8) is placed at a
takeoff when peak roll over the first third of the flight exceeds 20° on the fast lap
(`analysis.rs:2212-2217`), and the `scrub` finding doubles its score.

What the video adds is the two *purposes*, neither of which we model:

- **Turning jumps, stay low** — crank it over off the lip to flatten the trajectory, land
  earlier and get on the brakes sooner. Our `scrub` tip observes the symptom (more airtime,
  higher than the reference) but nothing connects it to the braking zone that follows.
- **Landing cranked on purpose** — when you know you're short, land still leaned so the rear
  slides up the face instead of compressing and rebounding.

I thought this second one might collide with `land_crooked` ⚠ (>15° of lean 4 m after touchdown).
It doesn't, in practice: `land_crooked` has no arm in `answers()`, so it is a review-only safety
note and never becomes a cue. Worth still putting real laps through it, because a rider who reads
their review and sees a warning for the thing that just worked will not trust the next one.

Confirmed in passing: "the less you do to the bike the better, the game straightens it out, and
fighting it makes it worse" is precisely what `bar_fight` measures.

## 2. Trail braking with the throttle on — a real gap, and the cheapest new rule

The claim: hold a little throttle *while* braking over braking bumps. Braking alone locks the
rear and you become a passenger; overlapping the two keeps the rear planted, lets the suspension
work, and makes the entry repeatable lap to lap.

Coach has `brake_early`, `brake_late`, `brake_harder`, `brake_unneeded`, `more_front`,
`coasting`, `clutch_braking`, `front_lock` — and nothing that looks at brake and throttle being
applied *together*. `coasting` measures the opposite case (neither input). Both channels are
already on the 1 m grid as `Point.brake()` and `Point.throttle`, so the overlap is directly
measurable with no new telemetry.

Shape of the work: a `trail_brake` corner finding, plus one line in `answers()` mapping it to
`BRAKE`. No plugin change, no new voice clip, no wire-format change.

Caveat worth keeping: this is a technique claim from one fast rider, not something we have
measured. Check it against the reference laps we already hold first — if our fastest laps don't
actually show the overlap, the rule doesn't get written.

## 3. Double upshifting — the symptoms are covered, the cue shape is not

The case: without quick-shift each auto-clutch shift costs about half a tenth, you shift 50-100
times a lap, so skip a gear where you'd otherwise hit the limiter or shift again mid-corner.
450 technique; a 250 would bog.

Both symptoms are already detected from the other end — `gear_up` / `gear_down` (wrong gear at
the apex and a slower exit) and `shift_earlier` (time on the limiter). And the code already
made the relevant decision on purpose: `cue_points()` deliberately never emits `UPSHIFT` or
`DOWNSHIFT` from the fast lap's own shift timing (`analysis.rs:2203-2206`); they come only from
`gear_up`/`gear_down`. That decision came from rider feedback — a cue names a gear to be in,
never a moment to change, because riders read "shift earlier" as "go slower".

A double-upshift cue is a *moment* cue by nature, so it argues with that. The right home is a
whole-lap tip ("you're shifting twice out of Turn 4, take third") rather than a new live cue.
Note also `shift_earlier` currently maps to no cue kind at all.

## 4. Line choice — our strongest research, and the plumbing is missing

This is where the video and Coach agree most, and where the cheapest real win sits.

The video's two halves are both already modelled:

- **Ride around the braking bumps and cut in late.** The video explains *why* the inside is
  bumpy (creators bump it deliberately to balance inside against outside). Our `line` finding
  names a side at the apex and `lines.rs` places the crowd's line across a session.
- **Take the slow line now for the fast exit later**, and the line nobody else takes doesn't get
  beaten up over the night. That is our "second line" rule and our `wear` notes, near enough
  word for word.

So nothing to research. But the inventory turned up that three positional side-of-track results
never reach a cue at all:

- `jump_line` (`analysis.rs:1338`) names a side on a jump — exactly `WIDE`/`INSIDE` shaped.
- `height` (`analysis.rs:1185`) names a high or low line.
- Every `lines.rs` `Note` (the whole multi-lap line and rut analysis) feeds the Lines page only;
  `cues::pick` takes `&[CuePoint]` and `&Review` and never sees it.

`from_finding()` only has an arm for `"line"`, so the other two are silently dropped. On top of
that, `WIDE` and `INSIDE` appear in no `answers()` arm, so they can never earn the ×2 score
bonus that every other cue kind can — the line calls are structurally handicapped in the ranking
against brake and throttle calls.

That is the single highest-value item here: it is a few arms in one match statement, it uses
analysis we have already written and validated, and it aims at the technique a pro-series rider
spends a third of the video on.

## 4c. Consistency — already on our list, now with a framing

Five steady laps beat two fast, two crashes and one average, because total race time is what
counts. COACHING.md's "Not yet" already names consistency beyond section spread, and
`Ideal` / `SectionBest.spread` (`analysis.rs:2108-2127`) is computed and unused. The video's
contribution is the framing: compare *total* time over N laps, not best lap.

## 5. Starts — a gap, and blocked at three layers

COACHING.md's "Not yet" lists "Starts (launch, wheelie, bog)" with no detail. The video gives a
step-by-step procedure, and it's the one part it claims as a genuine secret.

The raw telemetry is nearly all there — gear, throttle, clutch, rpm, rear slip, front wheel off
the ground, the stance bind, and every other bike's position. But three structural things block
it, and they are worth being clear about before anyone estimates this:

1. **Out-laps are excluded before analysis begins.** The whole pipeline is built on whole laps
   cut at the line and resampled along the centreline. A start is not a lap. This is a new
   section type before it is a new rule.
2. **The plugin gates cues out of races anyway** — cues show only in testing or a race event's
   practice session, so as things stand the gate drop is unreachable by the cue path.
3. **No cue kind fits.** `Kind` runs BRAKE(1) to CUSTOM(11) with one voice clip per kind, BRAKE
   to SIT. A start cue needs a new kind, a new clip from `tools/voice/make_clips.py` and a
   FrostMod version bump. There is room — the clip resource id is `100 + voice * 20 + kind`, so
   the stride allows up to 19 kinds — but it is a contract change across two repos.

And the input the video says matters most, the lean point, is the one we can't read: rider lean
needs a memory read and isn't in the plugin API. We can coach around it (gear, clutch timing,
push-forward, the stand-sit, wheelie and bog from the front wheel and revs) but not the thing
itself. Whether the push-forward input even reaches telemetry is unchecked.

## One more thing the video points at

Every cue's text is a single fixed string per kind (`cues.rs:78-93`) — every `BRAKE` cue says
"Brake here", on every corner, every lap. Magnitude exists in `Finding.detail` and dies there;
the wire format carries only `at`, `kind`, `priority` and `text`. That is the mechanical reason
behind the round-2 rider complaint that the cues are vague and identical every lap. The video is
specific about *how much* and *where* throughout, which is a decent argument for getting
magnitude onto the wire eventually.

## What is worth doing, in order

1. **Wire the line findings that already exist into cues** — `jump_line` and `height` arms in
   `from_finding()`, and `WIDE`/`INSIDE` arms in `answers()` so line calls can rank fairly.
   Cheapest change, best supported by both our research and the video.
2. **Check the trail-braking claim against our reference laps.** If it holds, a `trail_brake`
   finding plus one `answers()` arm on `BRAKE`.
3. **Consistency over a session**, using total time rather than best lap. The data is computed
   already.
4. **Put real laps through `land_crooked`** to see whether a deliberate cranked landing reads as
   a safety warning in the review.
5. **Starts** — the biggest by a distance, crossing into FrostMod, with the key input unreadable.

1 to 4 are Coach-side only. 5 is a two-repo change.
