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
   ≥ 0.25 s), rhythm (two jumps within 12 m) and whoops (three or more), and straights between.
5. **Explain** each section that loses more than 0.05 s with the rules below. The three
   sections losing most are shown first. Safety findings (front lock, crooked landing) show
   even where no time was lost.

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
| Jump | `jump_it` | reference jumps, lap rolls it |
| Jump | `chop_face` | throttle drops > 0.3 on the last 15 m of the face |
| Jump | `scrub` | > 10% + 0.1 s more airtime and > 0.5 m higher |
| Jump | `land_short` / `overjump` | lands > 2 m short / > 3 m long |
| Jump | `land_throttle` | off the gas at touchdown, reference on it |
| Jump | `land_crooked` ⚠ | > 10° of lean at touchdown |
| Whoops | `whoops_speed` / `whoops_throttle` / `whoops_bucking` | slower, off the gas, pitching more |
| Straight | `shift_earlier` / `full_gas` | more time on the limiter / less throttle |

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
