# MXB Coach — how a lap is reviewed

MXB Coach compares a lap with a faster **reference lap** on the same track (by default the
fastest whole lap on that track, same bike first) and says where the time went and why.

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
5. **Explain** each section that loses more than 0.05 s with the rules below. The three
   sections losing most are shown first. Safety findings (⚠: front lock, crooked or hard
   landing, sliding front) show even where no time was lost.

## Rules

Thresholds are starting values in `analysis.rs` → `mod th`, to be tuned on real laps.

| Section | Rule | Fires when |
|---|---|---|
| Corner | `brake_early` | brake onset > 5 m before the reference |
| Corner | `brake_late` | onset > 5 m after, and minimum speed < 95% |
| Corner | `brake_harder` | peak decel < 70% of the reference and zone > 120% as long |
| Corner | `brake_unneeded` | brakes where the reference doesn't |
| Corner | `more_front` | front share of braking < 50% where the reference's is ≥ 60% |
| Corner | `carry_speed` | minimum speed < 95% of the reference |
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
| Jump | `scrub` | > 10% + 0.1 s more airtime and > 0.5 m higher |
| Jump | `land_short` / `overjump` | lands > 2 m short / > 3 m long |
| Jump | `land_throttle` | off the gas at touchdown, reference on it |
| Jump | `land_crooked` ⚠ | > 15° of lean 4 m after touchdown (a whip has unwound by then), 7.5° more than the reference |
| Jump | `land_hard` ⚠ | the landing hits > 10 G and > 1.4× the reference (on its own: > 12 G) |
| Whoops | `whoops_speed` / `whoops_throttle` / `whoops_bucking` | slower, off the gas, pitching more |
| Straight | `shift_earlier` / `full_gas` | more time on the limiter / less throttle |

## Setup

Over the whole lap, with the fix for each in `fixes.rs` (the changes, in the order to try them,
written into a copy of the rider's setup where the bike's own option list for the setting is
known; geometry stays advice).

| Tip | Fires when | Fix |
|---|---|---|
| `setup_bottoming_fork` / `_shock` | ≥ 3 bottom-outs a lap | fork: compression, oil, spring; shock: high-speed compression, spring, preload |
| `setup_bottoming_shock_slow` | the shock's bottom-outs mostly compress slower than 0.4 m/s | low-speed compression, spring, preload |
| `setup_stiff_fork` / `_shock` | never past 70% of the travel | softer compression, oil or spring |
| `setup_brake_dive` | fork past 85% braking into ≥ 2 corners | fork compression, oil, preload |
| `setup_exit_squat` | shock past 75% on the gas out of ≥ 2 corners | shock low-speed compression, preload |
| `setup_shock_kick` | rear extends faster than 0.6 m/s at the lip on ≥ 2 jumps | slower shock rebound |
| `setup_packing_fork` / `_shock` | in whoops, > 50% deep on average and never extending faster than 0.4 / 0.2 m/s | faster rebound, softer compression |
| `setup_rear_low` / `setup_front_low` | on steady straights the shock sits > 30% deeper than the fork / the fork deeper than the shock | preload |
| `setup_front_push` | the front slides in ≥ 2 corners | softer fork compression, more shock preload |
| `setup_sag_rear_deep` / `_high` | standing still ≥ 1 s with the rider on, the shock outside 30–36% of its travel | shock preload by the millimetres it's off; the spring when preload runs out |
| `setup_pressure` | a tyre > 10 kPa from the `OptimalPressure` in its `.tyre` file | back to the tyre's optimum |
| `setup_gearing_*`, `setup_shift_*`, `setup_swingarm` | limiter, bogging, shift points, front up on exits | rear sprocket; swingarm as advice |

The suspension, acceleration and bar thresholds come from real laps (2026-09-15, five laps of a
250F): a landing's hit has a median of 5 G and a 90th percentile of 10 G, the bars into a
corner a median of 17. The game's brake pressure channel carries no data, so braking is read
from the lever inputs. The OEM MX tyres don't heat or wear in the game (heating factors and wear
rate are 0), so pressure is judged against the tyre's optimum only. Sag is measured standing
still when the recording has a second of it; riding sag is shown but not held to a target.

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

- Rider lean / body position (needs a memory read; not in the plugin API).
- Riding-aid settings and deformation from `profile.ini`, stored with each session.
- Starts (launch, wheelie, bog) and consistency across a session beyond section spread.
- A terrain background under the track map.

## Sources

Transmoto (braking, corner entry, jumps), Gary Semics (braking, clutch, scrubbing), Shane
Watts (tight turns), MXA (momentum, shifting, ruts), MotoOnline / CRA (whoops), Cycle World
(starts), MX Bikes on X (scrub method), MX Bikes Steam threads and forum (lean, aids, gearbox
preload, deformation), PiBoSo's `mxb_example.c` (the API).
