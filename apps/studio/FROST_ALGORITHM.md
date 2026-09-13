# Frost algorithm versions

The version stamped into every generated track (`<slug>/frost-algorithm.ini`, and the README).
Minor for a new feature, patch for a fix, 0.x until the generator is finished. Bump
`FROST_ALGORITHM_VERSION` in `tracksynth.rs` and add a row here with every generator change.

| Version | Date | Commit | Change |
|---|---|---|---|
| 0.28.0 | 2026-09-13 | 44824b98 | bigger ARL jumps, ruts over jumps, more braking bumps |
| 0.27.1 | 2026-09-13 | e2e2baee | fill the straights before the finish, shorter taller step-ups |
| 0.27.0 | 2026-09-13 | fa4632ce | paddock with team areas, pit road, start-gate sponsor wall |
| 0.26.0 | 2026-09-13 | 8c61f159 | taller jumps, full-width braking bumps, turn markers, fenced start |
| 0.25.0 | 2026-09-13 | e1a37319 | straight-line landings, pit area, ARL bumps and jumps, arches that draw |
| 0.24.1 | 2026-09-13 | 82eb6017 | bare sweepers, singles before turns, landings clear of turns, ARL bumps |
| 0.24.0 | 2026-09-12 | 785beee1 | rider round 9 - face ruts on corner lanes, no trench, short straights, roughness knob |
| 0.23.2 | 2026-09-12 | 11c66de4 | plant only library trees that show on their sheet |
| 0.23.1 | 2026-09-12 | 10607eca | track picture frames the tallest jump, less haze, mipped models |
| 0.23.0 | 2026-09-12 | bcabe25e | track picture is a close-up of the finish with scenery and title |
| 0.22.1 | 2026-09-12 | c92b2a86 | real doubles, steeper tabletops, ARL roughness, banners off the track |
| 0.22.0 | 2026-09-12 | 27b79d06 | air take-offs for doubles, triples and singles |
| 0.21.0 | 2026-09-12 | 4cb6f0ee | draw the track-info map as an overhead of the real ground |
| 0.20.5 | 2026-09-12 | d4429588 | lift donor props relative to the donor's ground and drape them |
| 0.20.4 | 2026-09-12 | 19e511a1 | arches on two legs, even lead-in ruts, steeper take-offs |
| 0.20.3 | 2026-09-12 | be1cc1f7 | place donor arches across the track instead of removing them |
| 0.20.2 | 2026-09-12 | 1c2f9971 | lead-in ruts bend into corners, face ruts run through the lip |
| 0.20.1 | 2026-09-12 | 602fa9a9 | keep big donor structures away from the track |
| 0.20.0 | 2026-09-12 | fddfbd03 | lead-in ruts, ground-level track, side singles, banner fix |
| 0.19.0 | 2026-09-12 | 9ed7e4b9 | bigger jumps, wet main lines, sharper tyre marks, map fix |
| 0.18.3 | 2026-09-12 | 4a2ef954 | braking bumps that roll, and start ruts as a comb |
| 0.18.2 | 2026-09-12 | 2a753e9a | tyre lines back, a start straight worked like track, ride fixes |
| 0.18.1 | 2026-09-12 | db3b0663 | a start straight that narrows, merges smoothly and looks like track |
| 0.18.0 | 2026-09-12 | bd734e03 | real objects only, finish arch on the jump, more ruts and jump types |
| 0.17.0 | 2026-09-12 | b4d7179d | light generated tracks like published ones, and drop tear-offs |
| 0.16.1 | 2026-09-11 | c48ef3ac | lifted scenery wears its own sheets, trees see-through |
| 0.16.0 | 2026-09-11 | 4e14b90e | a backdrop beyond the plot, a thicker wood, scenery at published distances |
| 0.15.3 | 2026-09-11 | 93af1115 | pass over drawn laps whose start straight lands on the lap |
| 0.15.2 | 2026-09-11 | 94e12c13 | scenery with its own textures, ruts that hand over, a touch more lip |
| 0.15.1 | 2026-09-11 | 7e448c0e | braking bumps as tall as Indiana's |
| 0.15.0 | 2026-09-11 | 5eef5bce | each braking bump its own size and shape |
| 0.14.0 | 2026-09-11 | 65462e2a | braking bumps form in staggered lines, not ridges across the track |
| 0.13.0 | 2026-09-11 | 0e582d8e | pebbles, a finer riding-surface mask, 70% jumps, bumps you can feel |
| 0.12.0 | 2026-09-11 | 4be907e8 | bake the ground's own relief into the riding surface's normal map |
| 0.11.0 | 2026-09-11 | ecbf440f | rut lanes that wander, come and go, and vary in width |
| 0.10.0 | 2026-09-11 | c657623a | braking bumps in sets of long waves, clear of the ruts, painted |
| 0.9.0 | 2026-09-11 | adc70206 | tight corners carry stated rut lanes |
| 0.8.0 | 2026-09-11 | 0175d452 | taller jumps with gentler faces, ruts that run through turns, darker dirt |
| 0.7.1 | 2026-09-11 | 33b089ec | keep Indiana's specular with its normal maps |
| 0.7.0 | 2026-09-11 | 2ac7c09d | photographed ground takes Indiana's own normal maps |
| 0.6.0 | 2026-09-11 | a553ff0e | more ruts per corner, and ground that catches the light |
| 0.5.1 | 2026-09-11 | 197f8224 | take-offs curve up then run straight to the lip |
| 0.5.0 | 2026-09-11 | de7ab023 | a longer, gentler finish face; ruts across more of a corner; braking bumps |
| 0.4.0 | 2026-09-10 | 9c2ec959 | takeoffs concave to the lip and flush with the deck; longer finish deck |
| 0.3.0 | 2026-09-10 | 4c75320a | jumps at 85% of their old height, the finish jump at full size |
| 0.2.4 | 2026-09-10 | 06e7b482 | scuff only the takeoff, and ease a jump's hold on the ruts in |
| 0.2.3 | 2026-09-10 | 8a7e9e35 | ease the jump-face paint in, and widen the tyre lines |
| 0.2.2 | 2026-09-10 | 663ac80e | paint the light dirt on the track only, and blend the ruts |
| 0.2.1 | 2026-09-10 | 8a65b310 | cap rut depth and thin the ruts in a corner |
| 0.2.0 | 2026-09-10 | 38ae2ce3 | middle-ground ruts, smaller jumps with a kick, darker riding surface |
| 0.1.2 | 2026-09-10 | 63cf9cac | the ground picker changes the ground, not the track |
| 0.1.1 | 2026-09-10 | cd63d835 | take the chop and the rut height out of generated ground |
| 0.1.0 | 2026-09-10 | 4c91ed95 | stamp the track generator version into built tracks |
