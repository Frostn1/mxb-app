# The real-track corpus

What real motocross tracks measure, and what each number becomes in a track program.

Two things live here. **Part 1** is the corpus: real-world numbers with the source they came
from, gathered because a generated track can only be judged against something. **Part 2** is
the translation — the same quantities in the vocabulary of `trackprog`, because a real track
and a track program do not measure the same things with the same words, and most of the errors
this document exists to catch are that mismatch rather than a wrong value.

The rule throughout: **every number carries its source.** Where a figure could not be sourced
it says so rather than being estimated. Numbers we measured ourselves off published MX Bikes
tracks are marked `[ours]` and come from `trackstats`; they are not real-world evidence, they
are what the game's own content does.

---

## Part 1 — What real tracks measure

### 1. What the rulebooks actually specify

The first finding is a negative one, and it shapes everything after it.

**FIM regulates almost nothing about jumps.** Article 4.10 "Jumps" of the FIM Motocross &
Supermoto Circuit Rules carries no numbers in the 2023, 2024, 2025 or 2026 editions — no
height, no length, no face angle, no landing slope, no spacing, no blind-jump rule. Its five
clauses are qualitative ("must be initially designed with the riders' safety in mind", "special
consideration must be given to the angle of the track at jump faces and landing zones"). **AMA
Pro Motocross has no track specification at all**, and the AMA amateur rulebook states outright
that "The AMA does not homologate racetracks."

So the shape of a jump is not a regulated quantity anywhere in the two championships the game's
content is modelled on. It is builder practice, and it has to be sourced as such (§4).

What *is* regulated is the envelope: how long a lap is, how wide, how fast it may average, and
where the gates go.

#### 1.1 FIM — MXGP/MX2/WMX/Junior/MXoN (2026 edition, current)

| Parameter | Value | Article |
|---|---|---|
| Course length | **min 1.5 km, max 2 km**, measured along the centre line | 4.2 §3–4 |
| Course width (actual riding width) | **min 6 m** (7 m sidecar/quad) | 4.2 §5 |
| Max average speed | **65 km/h**, averaged over a complete race | 4.1 §5 |
| Vertical clearance over the course | min 3 m | 4.2 §6 |
| Safety zone, course edge to fence | min 1 m, never less | 4.2 §7 |
| Earth banking defining the riding width | min 50 cm | 4.2 §8 |
| Course markers | 5–10 cm wide, **not over 50 cm tall** | 4.3 §2 |
| Starting gate width | **min 40 m** | 4.7 §8 |
| Gate positions | **40, one row, 1 m each** | 4.8 §6 |
| Gate height | 50–52 cm; concrete base ≤ 60 cm | 4.7 §4 |
| Distance, released gate to rear barrier | 2.5 m | 4.8 §10 |
| Start straight | **recommended 80–120 m**, gate to where the inside of the straight turns into the first bend | 4.9 §1 |
| Jumps on the start straight | **none** | 4.9 §2, 4.10 §5 |
| Whoops / washboards | **not allowed** | 4.11 §1 |
| Rolling waves | **max 80 cm high, 8–11 m crest to crest**; none on a descent; "must not be sharp shaped" | 4.11 §3–6 |
| Spectator metal fences | ≥ 2 m from the course delimitation, ≥ 2 m high | 7.1 §5, §7 |
| Watering | whole course evenly in **10–15 minutes** | 5.2 §2 |
| Surface | natural (sand, dirt); concrete and paving forbidden | 4.1 §2, §6 |

Source: FIM, *Motocross & Supermoto Circuit Rules*, Edition 2026 (update 21 January 2026) —
https://www.fim-moto.com/fileadmin/user_upload/Documents/2026/2026_MOTOCROSS_SUPERMOTO_CIRCUIT_RULES.pdf

**Drift worth knowing when calibrating against older tracks:** FIM ran **5 m** minimum width and
**1.75 km** maximum length through the 2019 edition, and a **55 km/h** average-speed cap in 2011.
Rolling-wave spacing was 8–10 m through 2025 and became 8–11 m for 2026.

#### 1.2 Motorcycling Australia — the only published standard with jump geometry

MA's *Standards for the Inspection and Licensing of Tracks* (v2024-01-26) is a real homologation
checklist from an FIM-affiliated federation, and it is the only document found anywhere that
puts numbers on a jump.

| Parameter | Motocross | Supercross |
|---|---|---|
| Track length | 1.5–3.0 km national; 800 m–3.0 km otherwise | ≥ 400 m outdoor, ≥ 300 m indoor |
| Track width | **7 m** (40 gates), 6 m (30 gates) | 6 m outdoor, 5 m indoor |
| Start straight to inside of first corner | **70–125 m** | 30–80 m |
| Min lap time | not specified | **35 s outdoor, 25 s indoor** |
| Max average speed | **65 km/h** | 65 km/h |
| **Jump height** | **max 3.0 m** | max 3.0 m |
| **Jump landing** | **1 m wider than, and in a straight line with, the take-off; well-rounded without a peak top; long gentle slope** | same |
| **Double** | **second jump 400 mm lower than the first** | take-off any height, ≥ 5 m wide |
| **Tabletop deck** | **3–21 m of flat** | 3–21 m |
| **Whoops** | **second half of the lap only, 3–6 m crest to crest, max 600 mm**, and a rider must not clear more than one at once | same, max 600 mm |
| Stutters | **banned** | max 1 m, 1–3 m spacing |
| Triples | **banned** | allowed; first jump highest, third lowest, third ≥ 1.5 m wider with twice the landing deck |
| Take-off ramp | **consistent gradient, no ruts or ledges** | same |
| Step-up | section between the ramps filled level with the top of the lower ramp | — |
| First corner | ≥ 12 m wide at entry, tapering to 8 m through it | ≥ 9 m wide |
| Start straight width | **hold the gate width for the first 50%**, then taper | flat and smooth to 5 m past the first corner exit |
| "Curve", defined | a direction change **> 15° with a radius under 300 m** | — |
| Hazard exclusion | 3 m from the track edge, 3 m above it, **8 m behind a berm** | same |
| Neutral zone | 4 m; **6 m where speeds exceed 60 km/h** or beside a tabletop; 8 m behind a berm | same |
| Rider-count formula | **N = W × L / 30 ± 1** (W = width of first corner, L = length of start straight) | same |

Source: Motorcycling Australia, *Standards for the Inspection and Licensing of Tracks*, v2024-01-26.

**The one sightline rule found in any rulebook** is MA's supercross triple clause: "The third
jump should be high enough to enable the rider to sight it."

#### 1.3 AMA

| Parameter | Value | Source |
|---|---|---|
| Pro Motocross track specification | **none exists** | AMA Pro Racing rulebook (2024) |
| Amateur course length | **½ – 1½ miles** (805–2414 m) | AMA Racing Rulebook 2025 |
| Amateur minimum width | **20 ft (6.1 m)** | AMA Racing Rulebook 2025 |
| Amateur gate space per bike | **1 m (3.2 ft)**; front wheel within 12 in of the gate | AMA Racing Rulebook 2025 |
| Homologation | "The AMA does not homologate racetracks." | AMA Racing Rulebook 2025 |
| Supercross narrowest point | 20 ft (6.1 m) | AMA Supercross rulebook 2026 |
| Supercross gate | min 80 ft (24.4 m); start area min 120 ft (36.6 m) long; 22 riders | AMA Supercross rulebook 2026 |


---

### 2. Real venues — what is actually published, and what is not

**US outdoor venues do not publish track geometry.** The official `promotocross.com` track page
for all twelve national venues was fetched and none carries a dimension. The 2026 AMA Pro Racing
rulebook contains no track length, width or jump specification. So the American series — the one
most game content is modelled on — is the *worst* documented, and anything asserted about a
national's layout beyond the numbers below is not sourced.

Coverage across the twelve AMA venues: soil 12/12, lap times 12/12, acreage 9/12,
**length 6/12 (one official), turns 2/12, per-lap elevation change 0/12.**

#### 2.1 Lap length

| Source | Value |
|---|---|
| **MXGP, official** — race distance ÷ lap count, 8 GPs, 2026 | **1.500–1.750 km** (0.93–1.09 mi) |
| MXGP Arnhem, official track page | 1.660 km |
| AMA venues, media | 1.0–1.5 mi (1.6–2.4 km); only Hangtown's "just over a mile" is official |
| FIM regulation | 1.5–2.0 km |
| MA regulation | 1.5–3.0 km national |
| `[ours]` published MX Bikes tracks | 1299–1767 m |

MXGP laps are consistently about a mile. The AMA media figures of 1.2–1.5 mi sit above that
band and are weakly sourced. **Budds Creek's widely-repeated "1.5 mile layout" fails a
cross-check** — with its sourced lap times it implies a 46.2 mph average, 24% faster than any
speed MXGP has ever recorded — and comes from an AI-generated aggregator. Do not use it.

#### 2.2 Speed — the official band

Signed FIM/Infront race classification PDFs, transponder timed, 8 GPs of 2026:

| | Range |
|---|---|
| **Winner's race average** | **48.944 – 54.690 km/h** (13.6–15.2 m/s) |
| **Fastest-lap average** | **50.390 – 59.949 km/h** (14.0–16.7 m/s) |
| Fastest lap in the sample | Riola Sardo (sand), 59.949 km/h |
| Slowest | Loket, 48.944 km/h race / 50.390 km/h fastest lap |
| FIM regulatory ceiling | 65 km/h average |

Source: `http://docs.mxgp.com/resultservice/ResPDF/2026/…` per-GP classification PDFs; corroborated at
`https://results.mxgp.com/reslists.aspx?e=<eventId>&c=9` (2026 MXGP event ids 4101–4118).

**There is no top-speed figure in motocross.** MXGP has no speed trap — this was positively
verified by enumerating every result type in the official timing system, not assumed. AMA timing
publishes sector splits but no speed column and no track length, so an AMA average speed cannot
be sourced at all. Stock-450 top speeds (KX450 83–94 mph, YZ450F 80 mph GPS) are media-only and
second-hand. **Corner speeds, jump take-off speeds and airtime: not found anywhere.**

#### 2.3 450 lap times, AMA nationals (official timing)

Best laps run **1:53.851** (Hangtown 2026, Jett Lawrence) to **2:23.895** (Fox Raceway 2024).
A full 30+2 moto is **15–18 laps**. Per venue, fastest sourced 450 best lap:

| Venue | Best lap | Venue | Best lap |
|---|---|---|---|
| Hangtown | 1:53.851 | RedBud | 2:07.062 |
| Budds Creek | 1:56.990 | Ironman | 2:10.686 |
| Loretta Lynn's | 1:57.122 | Washougal | 2:14.679 |
| Spring Creek | 1:58.763 | Unadilla | (see file) |
| Southwick | 2:00.728 | Fox Raceway | 2:23.895 (slowest) |
| Thunder Valley | 2:02.514 | High Point | 2:03.761 |

#### 2.4 Turns per lap

Only two venues publish one: **High Point 18, Ironman 16** (both from the same Wikipedia infobox
family, so effectively one source). MA defines a "curve" as a direction change **greater than 15°
with a radius under 300 m** — which is the only definition of a corner in any rulebook.

`[ours]` published MX Bikes tracks measure 13–25 corners of 25°+ per lap. That is consistent with
16–18, given the different counting threshold.

#### 2.5 Elevation

**No venue publishes a per-lap climb or drop.** Only two named features anywhere have a measured
height: **Budds Creek's downhill at 112 ft (34 m), land-surveyed**, and **Spring Creek's Mt. Martin
at 300 ft (91 m) of climb** (official).

Site altitude runs 40 ft (Budds Creek) to **6,400 ft (Thunder Valley — "the highest professional
motocross event in the world", official)**. Altitude is a power correction, not a layout
parameter, but Thunder Valley is a six-fold outlier and the only venue where it matters.

#### 2.6 Soil — the one field with full coverage

| Family | Venues |
|---|---|
| Deep sand | Southwick ("the most famous sandbox in American motocross", hard base), Loretta Lynn's |
| Sandy loam over clay | RedBud (clay base, trucked-in sand "blended to a loamy perfection"), Spring Creek, Budds Creek |
| Clay | High Point, Ironman |
| Loam over clay | Washougal ("a clay base with a loamy top layer"), Unadilla |
| Hardpack + imported soil | Hangtown ("the rock-hard base has been a constant"), Thunder Valley ("incredibly hard packed" natively, "tons and tons of sand" added) |
| Screened loam | Fox Raceway |

**Three of the twelve are engineered surfaces, not native ones** — Hangtown, Thunder Valley and
RedBud all sit on a hard native base with soil or sand trucked in and blended. The racing surface
is imported.

#### 2.7 Three premises that turned out to be false

Worth recording because they are the kind of thing that gets repeated into a prompt:
**LaRocco's Leap is at RedBud, not Millville** (Spring Creek's hill is Mt. Martin); RedBud's
**"Leap of Faith" does not exist** in any source; and **"the Washougal Wall" is a conflation** —
The Wall is at Unadilla.

---

### 3. Supercross — the one discipline whose builder publishes numbers

Supercross is not what we generate, but it is the best-documented dirt track in the world, and it
is the only place a professional builder has published a spec sheet. Where a quantity is missing
for outdoor MX, an SX figure at least brackets it.

#### 3.1 Dirt Wurx's own published spec (the complete list)

From dirtwurx.com, "Anatomy of a Supercross Track" — this is the *entire* set they publish. There
are no face angles, no ratios, no landing-slope method.

| | Value |
|---|---|
| Whoops, typical height | **3 ft (0.91 m)** |
| Triple jump | "**70 feet long and 35 feet high**" — see the warning below |
| Start | **22 gates, 80 ft (24.4 m) wide**; straight typically **150–375 ft (46–114 m)** |
| Structures per track | 9 |

> **Do not model the "35 feet high" triple as a 10.7 m lip.** It contradicts its own industry:
> the Jetwerx CEO puts the highest point of an entire track at about 15 ft. The figure is almost
> certainly the rider's trajectory apex, not the height of the ground. This is the single most
> common trap in real-world jump data — see §6.1.

#### 3.2 Whoops — the best-sourced obstacle in motocross

| | Value | Source |
|---|---|---|
| Height | **3 ft (0.91 m)** | Dirt Wurx; corroborated by WSX and Jetwerx |
| **Crest-to-crest, dozer-cut** | **14 ft (4.27 m)** | Dirt Wurx (Alex Gillespie, Racer X 2023) |
| **Crest-to-crest, loader-cut** | **13 ft (3.96 m)** | same |
| Design ideal | ~13 ft (3.96 m) | Feld ops director, SI 2023 |
| Number in a set | 10–14 (Jetwerx: "there'll be 11") | WSX |
| Section length | 80–120 ft (24–37 m) | WSX |
| MA regulation (MX and SX) | **3–6 m crest to crest, max 600 mm tall** | MA §6.7.7, §7.6.2 |
| FIM MX regulation | whoops **banned**; rolling waves 8–11 m at ≤ 80 cm | FIM 4.11 |

Quoted: *"Dozer whoops we usually cut tip to tip 14 feet apart, and the loaders are usually 13."*

**Whoop face angle is not published by anyone.** The builder explicitly frames steepness as an
emergent property: *"what makes them gnarly is how cupped out they get and how steep they get.
That's kind of out of our control on how they break down."* A generated whoop section should
therefore be built round-and-regular and allowed to degrade, not authored steep.

#### 3.3 Other SX obstacle geometry (WSX/Jetwerx typical ranges, not AMA)

| | Value |
|---|---|
| Berm height | 4–8 ft (1.2–2.4 m) |
| Berm radius | 25–60 ft (7.6–18.3 m) |
| Berm bank angle | **30–45°** |
| Rhythm lane length | 150–250 ft (46–76 m) |
| Tabletop length | 30–80 ft (9.1–24.4 m); one specific "20-foot tabletop" |
| Tabletop **height** and **deck length** | **not found — nobody publishes these** |

#### 3.4 SX scale, for contrast

| | Value | Source |
|---|---|---|
| Track width, narrowest | 20 ft (6.1 m) | AMA rulebook 2018/2020 |
| Course length | ≥ 300 m covered / 400 m open; ≥ 400/500 m for a World Championship | FIM Appendix 48 (2022) |
| Dirt per event | ~500 truckloads, ~26 million lb | Feld/SupercrossLive |
| **Measured 450SX lap times, 2025** | **46.710 s** (Salt Lake City) to **1:18.963** (Daytona); Anaheim 1 fastest lap 1:04.583 | official results |

**The "45–60 second supercross lap" is a design target, not a measurement.** Winner-time ÷ laps
gives Detroit 50.5 s, Indy 52.3 s, Anaheim 1 64.9 s, Daytona 82.4 s. The real spread for ordinary
stadium rounds is ~47–65 s.

#### 3.5 Arenacross — the only complete published spec set

Feld's arenacross press kit is the one source that publishes length *and* lap time together:
**~1,000 ft (305 m) in 18,000 sq ft (220 × 80 ft), lap time 28–38 s, ~30 ft/s (9.1 m/s)**,
170 truckloads of dirt. Its finish jump, "the Catapult", is quoted as *"nearly 30 feet in height
and over 50 feet in distance"* — again a **trajectory**, not a lip.

#### 3.6 Two numbers that circulate and should be rejected

The "**2½:1 / 2:1 supercross jump ratio**" and the "**4,000 yard track length**" trace to no
primary source. One published units error is also on record (a "5,500 cubic feet" dirt figure).

---

### 4. Jump geometry — what builders actually do

No motocross standard anywhere gives a takeoff angle. The dimensioned geometry lives in two
documents nobody cites — **Motorcycling Australia's** track-licensing standard and **Motorcycling
New Zealand's** MX Track Requirements — and they agree with each other independently and with
Dirt Wurx's built practice.

#### 4.1 The face

| | Ratio (run : rise) | Mean gradient |
|---|---|---|
| Motocross, distance-biased | **3 : 1** | **18.4°** |
| Supercross | 2½ : 1 | 21.8° |
| Supercross, steepest | **2 : 1** | **26.6°** |
| Certified FMX metal ramp (MA §14.5.1) | 2.26 : 1 | 23.9° mean, **42–45° exit** |

**Consolidated: a real jump face runs 2:1 to 3:1 — 18.4° to 26.6° of mean gradient.** Every
independent source lands inside that band.

The critical reading, and the thing that makes these numbers usable: **the ratio describes the
MEAN gradient of a concave face, so the lip is materially steeper than the ratio implies.** The
builder's phrase is *"your takeoff ramp slope should be around 3:1 with a slight bowl like curve."*
The certified FMX ramp makes the same relationship explicit — 6.1 m base, 2.7 m height, 9.1 m
constant transition radius, which solves to a 23.9° mean chord and a 42–45° exit. **A face's exit
angle is roughly 1.8× its mean gradient.**

Other face rules:
- The face must be **longer than the bike's wheelbase** (~1.5 m).
- *"The steeper the lip, the longer it has to be. The faster you're going, the longer it has to be."*
- Transition radius is G-limited: **`r_min = v² / (1.5 g)` = `v² / 14.7`**, i.e. 4.7 m at 30 km/h,
  13.1 m at 50 km/h, 22.2 m at the 65 km/h regulatory ceiling. A fast racing takeoff wants a
  transition radius on the order of **20 m** — more than twice the FMX ramp's, which is approached slowly.
- A worked example from one builder describing one jump: **takeoff 3.5 ft high × 10 ft long (19.3°)**.

#### 4.2 The landing

| | Value |
|---|---|
| Landing gradient, same jump as above | **3.5 ft high × 18 ft long = 11.0°** |
| Landing vs takeoff | **~60% of the takeoff gradient, ~1.8× as long for the same height** |
| Landing ramp ceiling (FMX ramp-to-dirt) | ≤ 45° |
| Landing shape (MA + MNZ, verbatim) | *"well-rounded without a peak top, with a long gentle slope for landing"* |
| **Landing width** | **+1 m wider than the takeoff, and in a straight line with it** |

**"+1 m wider" is the single most reliable jump-geometry number in existence** — three independent
federations (MA, MNZ, FIM Supercross) state it in almost identical words.

The design method itself is trajectory-matching: *"The goal is to create a landing where the impact
is nearly the same or does not surpass the EFH along the entire landing"* — equivalent fall height,
with a target around 4–5 ft. The practitioner version is blunter: *"build a lip, launch your test
bike, where it lands, that's where you build the landing."*

#### 4.3 Run-up — answered by a written standard

| | Value |
|---|---|
| **Minimum run-up before each dirt jump** | **≥ 20 m (66 ft)** |
| Metal ramp run-up | ≥ 20 m pro, ≥ 25 m amateur |
| And the other direction | *"The length of approaches to Jumps should be limited to control approach speed."* (MA and MNZ both) |

**Run-up is a two-sided device: long enough to clear, short enough to cap the speed.** That is
exactly the constraint `trackspeed`/`Rhythm` implements, stated as a rule by two federations.

Gap-to-speed, from projectile motion (`v = √(g·d / sin 2θ)`):

| Gap | at 25° lip | at 35° lip | at 45° lip |
|---|---|---|---|
| 10 m | 41 km/h | 37 km/h | 36 km/h |
| 15 m | 50 km/h | 45 km/h | 43 km/h |
| 20 m | 58 km/h | 52 km/h | 50 km/h |
| **24 m** (MA's gap ceiling) | **63 km/h** | 57 km/h | 55 km/h |
| 21 m (a 70 ft SX triple) | 59 km/h | 53 km/h | 51 km/h |

**The ~24 m gap ceiling and the 65 km/h average-speed ceiling are mutually consistent** — the
biggest jump a standard permits sits right at the speed a standard permits.

#### 4.4 Jump types

| | Value |
|---|---|
| Jump height ceiling | **3.0 m** (MA and MNZ) |
| Tabletop deck | **3–21 m** of flat |
| Double, second jump | **400 mm lower** than the first |
| Step-up | the section between the ramps is **filled level with the top of the lower ramp** |
| Triple threshold | > 21 m / > 0.6 m — which matches Dirt Wurx's built 70 ft (21.3 m) triple |
| Triples and stutters in MX | **banned** (allowed in SX) |

### 5. Corners, berms and ruts — the weakest-sourced area

This has to be said honestly: **MX berm geometry is essentially unpublished.** Every "a berm is X
feet tall at Y degrees" result on the open web traces to uncited SEO content that contradicts itself
(6–12 in vs 2–4 ft vs 3–5 ft; 25° vs 30–45°). No builder interview, federation standard or magazine
gives an MX berm angle.

What is actually sourced:

| | Value |
|---|---|
| Course-defining earth banking | **min 50 cm**, leading edge always round-shaped (FIM) |
| Berm regulation | by **outcome, not dimension** — MA regulates only that height, pitch and approach speed must not create a hazard |
| Clear zone behind a berm | **8 m** |
| **Ruts per corner** | **5–6 in every turn** (Jason Thomas, ex-pro) |
| **Rut depth** | **not found as a measured number anywhere.** Best proxy: the prep depth that produces them, **4–12 in (0.10–0.30 m)** ripped weekly |
| Rut formation | *"steep banks attacked at slow speed rut up deeply"*; deliberately cultivated — *"we rip as deep as possible to develop ruts"* |
| Sand vs hardpack | sand *"gives way, acting like a cushion"*; a harder base gives *"deeper ruts with more solid sides"* |
| **First corner width** | **≥ 12 m at entry, tapering to 8 m** — the only corner-widening number in any standard |
| **Field-size formula** | **N = W × L / 30 ± 1** (W = first-corner width, L = start-straight length) |
| "Curve", defined | > 15° direction change, radius < 300 m |
| Layout doctrine | *"designed with minimal stop / start turns"*, and *"to allow for safe passing"* |
| **MX corner radius, off-camber angle, rut spacing, rut width, berm height/angle/radius** | **not found** |

**Physics bounds where observation is missing.** Banking angle `arctan(v²/gr)`: at 30 km/h through
an 8 m radius, 41.5°; at 40 km/h through 10 m, 51.5°. Motocross berms are necessarily far steeper
than MTB berms (typical 25–30°), but no source states a real one, so these are bounds, not
observations.

**Inverting the field-size formula is the most useful corner relationship in the corpus:** a
40-rider gate on an 80 m start straight needs `W = 40 × 30 / 80 = 15 m` of first-corner width —
2.5× the 6 m minimum track width.

---

### 6. Measured jump profiles — the only engineering-grade data that exists

**There is essentially no peer-reviewed motocross jump kinematics literature.** Targeted queries
against Europe PMC and CrossRef return nothing that measures a motocross takeoff angle, takeoff
speed, flight time, jump distance or landing slope. This is a genuine gap, not a search failure.

What does exist is the **terrain-park equivalent-fall-height (EFH) corpus** — Hubbard, McNeil,
Swedberg and Petrone — and it transfers directly, because the governing equation contains no mass
and no gravity term (Petrone 2017, verbatim: *"the gravity constant g does not appear in Eq. 1"*).

#### 6.1 Takeoff angles measured on real built jumps

| Jump | Takeoff angle | Kind |
|---|---|---|
| California 2002 | 13° | ski, measured |
| Wisconsin 2015 | 13° | ski, measured |
| Colorado 2009 (professionally surveyed) | 16° | ski, measured |
| **Sydney 2020 dirt MTB jump** | **22.4°** | **dirt, surveyed** |
| Utah 2010 | 23° | ski, measured — *"EFH ≥ 1.5 m threshold for knee collapse no matter the takeoff speed"* |
| Washington 2004 | 25° | ski, measured — *"very large equivalent fall heights (3 m to 13 m)"* |
| BMX national start ramp | 28° | measured |
| Petrone 2017 (designed, built, jumped) | 10° | ski, design |

**The engineered ones cluster low (10–16°); the ones measured in the field and found dangerous are
the steep ones (23–25°).**

The Sydney 2020 jump is the only surveyed *dirt* jump profile in the open literature — lip **+22.4°**,
landing face steepest at **−21.5°**, total surveyed run 14.25 m.

#### 6.2 Four design rules from that literature

1. **Straight the last ~2 m of the lip** — *"the last 2 m of the takeoff were straight to avoid any
   potential inadvertent inversion hazard."*
2. **"Landing angle = takeoff angle" is explicitly wrong.** Swedberg 2010 rebuts it by name: it is
   what produces 3–13 m EFH on tabletops. The landing must be steeper than the flight path and
   monotonically decreasing in slope.
3. **Landing is buildable to about −30° maximum** (machine limit).
4. **Target EFH 0.5–1.0 m; 1.5 m is the absolute ceiling** (knee collapse in an adult).
   Note the 1.5 m is a conservative round-down adopted by the US Terrain Park Council — Minetti's
   own paper says 1.6–2.0 m sedentary, 2.6–3.0 m for athletes.

Transition radius from the same literature: a worked example gives **r_min = 12.7 m** at 13.7 m/s
under a 1.5 g limit, agreeing with the builder-side `r = v²/14.7`.

#### 6.3 Telemetry — what LitPro actually publishes

LitPro is the GPS box on the helmet, and it measures speed, jump height and distance, and
g-forces. Almost none of that reaches the public: **all 45 of their per-round "Moto Metrics"
race breakdowns carry lap times, sector times, "Lap 99" and consistency scores only** — no speeds,
no airtime, no jump counts, no g-forces.

The figures that have been published, all from LitPro's president directly:

| | Value | Context |
|---|---|---|
| Average speed through a jump section, **jumping** | 27.8 mph = **44.7 km/h** | 2019 Daytona SX, Baggett, 450 |
| Same section, **rolling** | 20.5 mph = **33.0 km/h** | same |
| Time gained by jumping it | **0.2-0.3 s per lap** | same |
| **Landing g-force** | **10.7 g** | same — described as unremarkable at pro level |
| **Rhythm-section transition g-force** | **18 g** | LitPro's own recorded ride |
| Corner entry to apex (illustrative) | ~50 mph to ~20 mph = **80 to 32 km/h** | LitPro's braking-visualisation example |
| **Start: 0.85 s to the 15 ft (4.6 m) mark** | 450, dirt start, "gold standard" | derived: **12.7 m/s2 (1.3 g) average, 39 km/h at 4.6 m** |
| 1st-to-4th margin, 2015 SX season | 0.965 s per lap | |

The corner figure is worth holding onto: **~32 km/h at the apex** is what our `A_LAT = 4.6`
produces for a 15-20 m radius, so the lateral model is in the right place.

**Radar measurements of national top speeds exist but the numbers were never written down.**
TransWorld ran a Bushnell radar gun at four 2018 Pro Motocross rounds (Thunder Valley, Southwick,
Washougal, Spring Creek) and at RedBud's LaRocco's Leap; Swapmoto ran two more at Fox Raceway.
Every one is video-only — no figure appears in any title, description, article body or caption.

#### 6.4 The signature jumps — real distances

These are the only outdoor-national jump dimensions published anywhere, and they are **distances
lip to landing**, not gaps of flat ground:

| Jump | Published distance |
|---|---|
| Fox Raceway (Pala) quad | "150+ foot" = **45.7 m** |
| RedBud, LaRocco's Leap | **120 ft (36.6 m)** per Wikipedia; *"125-plus foot"* per the promoter — the two disagree, as do the build years (1991 vs 1992), and neither states a method |
| Washougal, triple step-up (new for 2025) | "100+ foot" = **30.5 m** |

**Note these exceed MA's 24 m gap ceiling substantially.** That is not a contradiction — MA is an
Australian homologation standard and AMA homologates nothing — but it does mean a national's one
signature jump is bigger than any rulebook would permit, and our 15-25 m gap band describes the
ordinary jumps rather than the famous one.

#### 6.5 Engine output — and the absence of a top speed

**No published measured top speed exists for any stock 450cc motocross bike.** Not from the
manufacturers (zero "top speed" occurrences on official model pages), not from Cycle World, MXA,
Dirt Rider or Racer X. The only figures in circulation are an 80 mph *dyno* number with the rear
tyre free-spinning, and an MXA *estimate* of "around 65 mph" for a pro at Glen Helen.

Rear-wheel dyno output is well published: **50–60 hp (37–45 kW) for a modern 450**, e.g. 2026 KTM
450 SX-F 56.7 hp / 34.5 lb-ft, Yamaha YZ450F 53.5 hp, Honda CRF450R 52.6 hp (Dirt Rider, in-house
Dynojet 250i, rear wheel).

---

## Part 2 — The translation

Real-world numbers and track-program parameters do not measure the same things. This part maps each
one, and flags where ours sits outside what real tracks do.

### 7. Units and conventions

| | |
|---|---|
| All program lengths | metres; all angles degrees |
| Heading | 0 looks down +z, increases clockwise towards +x |
| Arc radius | **signed** — positive turns right |
| `terrain.scale` | the whole height budget; the 16-bit heightmap quantises against it |
| `terrain.samples` | a power of two plus one (2049 default). Indiana: 2049 over 525 m = **0.256 m/sample** |
| Real-world feet | the corpus above converts throughout; the program is metric only |

### 8. The four measurements that do not mean what they look like

These are where a real number gets mistranslated, and they cause more error than any wrong value.

**8.1 `width` is the graded corridor, not the riding width.**
FIM's "min 6 m" and MA's "7 m" are the **actual riding width** — the surface riders use. Our
`width` is the corridor the synthesiser carves, and the ridden file inside it is
`LINE_HALF_WIDTH_M = 2.1` either side, i.e. **4.2 m**, spreading 0.85 more through a corner.
So `width` 10–17 m does *not* contradict a 6 m regulation — they are different measurements. But
**our ridden line at 4.2 m is narrower than the 6 m minimum riding width any federation allows**,
and that is a real mismatch worth checking.

**8.2 "Jump height" in the press is a trajectory, not a lip.**
Dirt Wurx's *"a triple jump is 70 feet long and 35 feet high"* and arenacross's *"nearly 30 feet in
height"* are the **rider's apex**, not the height of the ground. `Feature::height` is the ground
height of the lip above the surrounding terrain. The regulated quantity — MA's **3.0 m ceiling** —
is the ground one, and is the one to compare against. Use published *lengths*, distrust published
*heights*.

**8.3 A builder's "2:1" is a mean gradient, not a lip angle.**
`JUMP_FACE_DEG = 27.0` is the angle **at the lip** of an arc that is tangent to the ground at its
foot. The builder's ratio describes the **mean** gradient of that same concave face. The two differ
by a factor of about 1.8–2.0 (our model: exactly 2.0; the certified FMX ramp: 1.82).

**8.4 A face angle measured off terrain is not either of those.**
`trackstats`' `jump_profiles` samples the synthesised ground, so it reports something between the
mean and the peak depending on the window. Indiana reads p50 12.0°, p90 27.4°. Compare like with
like or the numbers will seem to disagree when they don't.

### 9. Parameter-by-parameter translation

`[ours]` = measured off published MX Bikes tracks. **Bold** rows are where we sit outside the
real-world band.

| Program parameter | Ours now | Real world | Verdict |
|---|---|---|---|
| lap length (`LAP_M`) | 500–2600, meas 1299–1767 | MXGP **1500–1750** actual; FIM 1500–2000; MA 1500–3000; AMA amateur 805–2414 | agrees |
| `width` (corridor) | 8–20, meas 10.0–17.1 | riding width min 6 (FIM) / 7 (MA); built 7.6; first corner 12→8 | different measurement, §8.1 |
| **ridden line width** | **4.2 m** | **6–7 m minimum riding width** | **ours is narrow** |
| `TURN_RADIUS_M` | 7–30, meas 10.6–18.5 | **not published by anyone**; SX berm radii 7.6–18.3 | no real check available |
| `TURNS` per lap | 10–30, meas 13–25 | High Point 18, Ironman 16 (weak, one source) | agrees |
| `TOTAL_TURN_DEG` | 1400–3600, meas 1726–2960 | **not published** | game-side only |
| `ARC_FRACTION` | 0.5–1.0, meas 0.61–0.91 | **not published** | game-side only |
| `LAP_CLIMB_M` | 2–70, meas 2.2–66.3 | **no venue publishes a per-lap climb.** Only 2 measured features exist: Budds Creek 34 m, Mt. Martin 91 m | plausible, unverifiable |
| `FEATURE_HEIGHT_M` | 0.3–5.0 | **MA/MNZ ceiling 3.0 m** | **ours allows 67% over the regulated max** |
| `LIPS_PER_KM` | 12–45, meas 10–25 | **not published** | game-side only |
| `WHOOP_SPACING_M` | 2.5–8.0 (prompt: 4–6) | Dirt Wurx **3.96–4.27** built; MA standard **3–6**; FIM rolling waves 8–11 | agrees; 4–6 is right |
| whoop height | unconstrained | Dirt Wurx **0.91**; MA max **0.6**; FIM waves max 0.8 | should be capped |
| `TABLETOP_DECK_M` | 6.0 min, finish max 12.0 | **MA 3–21 m of flat** | agrees |
| double `gap` | prompt 8–12 small, 15–25 big | MA gap ceiling **24 m**; SX triple 21.3 m | agrees |
| `JUMP_FACE_DEG` | 27° at the lip → **13.5° mean** | builders **18.4–26.6° mean**; FMX 23.9° mean / 42–45° exit; Sydney dirt jump 22.4° lip | **ours is shallower than any built jump** |
| `JUMP_LANDING_DEG` | 19° → 9.5° mean | one builder's jump: 11.0° mean; ~60% of takeoff, 1.8× as long | ours is 70% and 1.44×; close |
| face length, 3 m jump | **12.5 m** | 2:1 → 6 m; 3:1 → **9 m** | **ours is 39–108% longer** |
| run-up before a jump | speed-checked per feature | **≥ 20 m written standard**, both directions | add the floor |
| `START_SPRINT_M` | 70.0 | FIM **80–120** recommended; MA **70–125** | at the floor of both |
| `START_LINE_M` | 150.0 | published MXB start lines 79–91 m of straight | **above both** |
| gate row | 40 stalls × 1.2 m = **48 m** | FIM **40 stalls × 1 m, min 40 m**; AMA 1 m | legal (min), 20% over |
| first corner width | not modelled | **≥ 12 m tapering to 8 m**; `N = W×L/30` | not modelled |
| landing width | not modelled | **+1 m wider than takeoff** (3 federations) | not modelled |
| `CORNER_BERM_M` | 0.55 self-formed | FIM course banking min 0.5; SX built berms 1.2–2.4 | agrees for MX |
| berm bank angle | not stated | **unpublished.** Physics: 41.5° at 30 km/h through 8 m | no real check available |
| rut depth | scale 0.86 → p50 ~0.21, p90 ~0.44 | **unpublished.** Proxy: worked layer ripped **0.10–0.30 m** | corroborated |
| ruts per corner | meas 2.9 across the line | **5–6 per turn** (ex-pro) | counts differ; check |
| `V_MAX` | 20 m/s = **72 km/h** | **no measured top speed exists.** MXGP fastest-lap *average* 50.4–59.9 km/h; FIM cap 65 km/h average | plausible ceiling |
| lap average speed | model implies ~60 km/h | **MXGP race average 48.9–54.7 km/h** | **ours is above the real band** |
| `P_SPEC` 22 W/kg | a 250's drive, to ground | 450 rear-wheel **37–45 kW**; code comment says 40 kW "at the crank" — that is the wheel figure | comment is slightly off |
| `A_TRACTION` 3.4 m/s2 | corner-exit drive | LitPro start: **12.7 m/s2 (1.3 g)** over the first 4.6 m | a gate launch, not a corner exit — but check |
| `A_LAT` 4.6 m/s2 | 20 m turn to 35 km/h | LitPro corner apex **~32 km/h** | agrees |
| landing impact | not modelled | **10.7 g** landing, **18 g** rhythm transition | not modelled |
| biggest jump on a lap | prompt 2.5-4.0 m, gap 15-25 m | signature jumps **30.5-45.7 m** lip to landing | ours describes ordinary jumps, not the famous one |
| `terrain.surface` | soil / sand / grass | 12 AMA venues: sand, sandy loam over clay, clay, loam over clay, hardpack+imported | 3 classes covers it |

### 10. What this changes — in priority order

1. **Jump faces are too long and too shallow.** A 3 m jump gets a 12.5 m face at 13.5° mean, where
   every real source says 6–9 m at 18.4–26.6°. The `face_run` half-angle relation is right and the
   arc shape is right — the exit/mean factor of 2.0 almost exactly matches the certified ramp's 1.82
   — so this is the **angle constant**, not the model. Raising `JUMP_FACE_DEG` shortens the face:

   | `JUMP_FACE_DEG` | face for a 3 m jump | mean gradient |
   |---|---|---|
   | 27° (now) | 12.50 m | 13.5° |
   | 34° | 9.83 m | 17.0° |
   | **40°** | **8.24 m** | **20.0°** |
   | 45° | 7.24 m | 22.5° |

   **But do not simply crank it.** A generated lap was rejected in the past for measuring p50 17.8°
   against Indiana's 12.0° — that is the terrain-sampled statistic of §8.4, over *all* features
   including rollers, and it is not the same quantity as the builder's mean gradient. The real
   finding then was that the spread comes from the face-length **floor** (`JUMP_FACE_MIN_M = 9.0`),
   which is what keeps small jumps long and low. Both can be true: the floor governs the rollers,
   the angle governs the big ones. Change the angle, keep the floor, and re-measure with
   `jump_profiles` **and** `corpus` against Indiana in the same run.
2. **Cap feature height at 3.0 m** for anything but an explicit supercross brief. Two federations
   set that ceiling; `FEATURE_HEIGHT_M` currently allows 5.0.
3. **Straight the last ~2 m of the lip.** `face_arc` is steepest exactly at the lip, which is the
   one thing the terrain-park literature says to avoid — it is an inadvertent-inversion hazard.
4. **Cap whoop height** at 0.6–0.9 m. Currently unconstrained; every source agrees on the figure.
5. **Add the 20 m run-up floor** per jump, alongside the existing speed check.
6. **Widen the ridden line** — 4.2 m against a 6 m regulated minimum.
7. **Check the speed model's average** against MXGP's 48.9–54.7 km/h rather than the assumed 60.
8. **`START_SPRINT_M` 70 → 80–120** to match FIM's recommendation, and `START_LINE_M` 150 is longer
   than any published start.

### 11. What cannot be checked against reality, and never will be

These are game-side only. No real-world source publishes them, so published MX Bikes tracks remain
the only oracle: **total turning, arc fraction, segments per lap, jumps per km, corner radius,
berm geometry, rut spacing and width, off-camber angle, per-lap elevation change, and MX cross-fall.**

For these, `trackllm::corpus` and `trackstats` are the authority and this document has nothing to add.

---

## Source register

Primary documents, all fetched and text-extracted rather than read from search snippets.

**Regulatory**
- FIM, *Motocross & Supermoto Circuit Rules*, Edition 2026 (update 21 January 2026) —
  `fim-moto.com/fileadmin/user_upload/Documents/2026/2026_MOTOCROSS_SUPERMOTO_CIRCUIT_RULES.pdf`
- FIM Appendix 048, Arenacross/Supercross, 2022 edition (dropped from FIM documents after 2024)
- Motorcycling Australia, *Standards for the Inspection and Licensing of Tracks*, v2024-01-26
- Motorcycling New Zealand, *MX Track Requirements*
- AMA Racing Rulebook 2025 and 2010; AMA Supercross rulebook 2026; AMA Pro Racing rulebook 2024/2026
- UCI *Cycling Regulations Part VI BMX* (2019) and *BMX Track Guide* (2017) — for contrast only

**Results and timing**
- FIM/Infront signed race classification PDFs, 2026 season — `docs.mxgp.com/resultservice/ResPDF/2026/`
- `results.mxgp.com/reslists.aspx?e=<eventId>&c=9` (2026 MXGP event ids 4101-4118)
- promotocross.com official timing, 450 class, 2018-2026

**Builder**
- Dirt Wurx USA, "Anatomy of a Supercross Track" — `dirtwurx.com`
- Alex Gillespie (Dirt Wurx) interview, Racer X, 2023 — the whoop-pitch source
- Feld/SupercrossLive FAQ; AMSOIL Arenacross press kit, 2013
- WSX / Jetwerx pre-event track descriptions

**Academic**
- Petrone, Cognolato, McNeil & Hubbard (2017), *Sports Engineering* 20:283-292, `10.1007/s12283-017-0253-y`
- Levy, Hubbard, McNeil & Swedberg (2015), *Sports Engineering* 18(4):227-239, `10.1007/s12283-015-0182-6`
- Swedberg (2010), *Safer Ski Jumps*, MS thesis, Naval Postgraduate School
- Moore & Hubbard, `skijumpdesign` documentation and the `sydney-measurements-2020.csv` survey
- Minetti et al. (1998), *Ergonomics* 41(12):1771-1791 — the origin of the 1.5 m EFH figure
- Ostermann et al. (2021), "Injuries in supercross", *J Clin Orthop Trauma*, PMC8008155

**Planning documents** (dimensioned real facilities)
- Riverside County CA, Environmental Assessment CEQ190083, "JS 63 MX", CUP 190014
- Snohomish County WA Code SCC 30.28.100/.105; CA State Parks OHMVR Mammoth Bar IS/MND 2020

### Things that were looked for and do not exist

Recorded so nobody spends the budget again: MX corner radius, berm height/angle/radius, rut depth
and spacing, off-camber angle, MX cross-fall, per-lap elevation change at any AMA venue, any
motocross jump face angle from a builder, rhythm-lane jump spacing, SX tabletop height and deck
length, and any measured top speed for a stock 450.

Two numbers that circulate widely and trace to no primary source — the "2.5:1 supercross jump
ratio" and the "4,000 yard track length" — should be rejected on sight, as should Dirt Wurx's own
"35 feet high" triple and Budds Creek's "1.5 mile lap".
