# Plan: recording rider lean

Written 2026-09-17. Route chosen: the controller axis now, the game's own value kept as a
fallback.

## Why it matters

Four of the five sources in [references/technique-gaps.md](references/technique-gaps.md) build
their core advice on rider body position — lean forward or back, counter-lean or stay central.
Lynds' whole cornering lesson is organised around it. COACHING.md has recorded rider lean as
out of reach since the beginning, because it is not in the plugin API, and that is still true:
every orientation value in the bike data is the chassis. There is no rider field of any kind, so
it cannot even be derived by subtraction.

But it is reachable, by the route the Sit control already uses.

## What the game gives us

MX Bikes has three separate lean controls, and only the first one is the bike:

| Control | What it moves |
|---|---|
| `CTRL_LEAN` | the bike — lean and steer, the left stick |
| `CTRL_FBLEAN` | the rider, forward and back |
| `CTRL_LRLEAN` | the rider, left and right — counter-lean |

Each is bound in the rider's `controls.txt`, one line per control, the same file the Sit
binding is already read from. An axis binding names the device and the axis number and a
direction flag, then ten tuning values. The manager already parses that line shape in
`feel.rs`, and the app already knows these three control names by name in the Feel editor.

There is an aids twin to the one we already handle: `[aids] autoriderlrlean` and
`autoriderfblean` in `profile.ini`, alongside the `autoridersit` we already honour. When either
is on, the game moves the rider itself and the axis says nothing.

## The route: the axis, through the path Sit already uses

`stance.h` already parses an `AXIS` binding — device GUID and axis number both land in the
`Bind` — and then `Rate()` discards it, because Sit only ever needed keys and buttons. The
plugin already opens the pad with the full joystick data format and reads a state struct that
already carries every axis. It simply never looks past the buttons.

So the work is small, and all of it is in code that exists:

1. **Accept the axis in `Rate()`** for the two lean controls, and honour the two aids flags the
   way `autoridersit` is honoured.
2. **Normalise the device** with a range property, so two pads give comparable numbers. Nothing
   sets one today, because a button needs none.
3. **The two-sided key case.** These controls are bipolar, so a keyboard rider binds two codes —
   one per direction. The parser currently keeps the first and drops the second, with a comment
   saying Sit only has one. Lean needs both, read as −1 / 0 / +1.
4. **Map the axis number to a member of the state struct.** This is the one piece that is not
   settled and **must not be guessed**: the game's axis number indexes its own per-device table,
   and the obvious ordering is only an assumption until a real pad proves it. Pin it by binding
   a known stick and logging every axis to see which one moves.
5. **A new record**, mirroring the Sit pair: the binding and tuning values once, then the two
   values per telemetry sample.

## Record the raw axis, not a derived one

The value the physics gets is not the raw axis — the game applies a deadzone, a linearity curve,
a gain, a direction flag and asymmetric smoothing, all from the ten tuning values on that same
line.

Record the **raw axis plus those ten values**, and derive later. Two reasons:

- A derived value bakes in today's arithmetic. If we get the curve wrong, every recording made
  before we notice is wrong with it. Raw plus parameters can be re-derived.
- Comparability. Against the rider's own fast lap — our default reference — the curve is
  identical on both laps and cancels, so the raw axis compares directly. Across riders it does
  not, and then the curve has to be applied. Recording both lets us do either.

## What this is, and is not

It is rider **input** — where the rider is asking to be. It is not a body angle. The rider's
body has its own dynamics in the bike's own config (lean rate, pitch spring, maximum angles),
and the aids can override it entirely.

For coaching that is mostly fine: "you are not counter-leaning here and the fast lap is" is an
input-level statement, and so is every piece of advice in the corpus. But no finding should be
worded as though we can see the rider's body, and any rule must treat an aids-on lap as unknown
rather than as zero.

## The fallback

The game computes each control's final post-curve value and keeps it with that control's entry,
which can be found by its own name rather than by a fixed address. Reading that gives the game's
exact number with no device matching, no range property and no curve to replay.

It is the fallback rather than the route because the Coach recorder reads no game memory at all
today — it takes everything through the published callbacks — and that property is worth keeping
while the cheaper route is untested. If the axis mapping turns out to vary by device in a way we
cannot pin, this is what we switch to.

## Then what

Recording is the whole of this piece of work. On the Coach side the values become two more
channels on the 1 m grid, and after that the rules are ordinary: counter-lean against the
reference through a corner, fore/aft through a braking zone, both gated on the corner's type
once [corner-types-plan.md](corner-types-plan.md) lands — because whether counter-lean is right
or wrong is exactly what corner type decides.

## What cannot be done from here

Steps 4 and the whole of verification need the game running with a pad on Windows. The code can
be written and compile-checked from macOS; it cannot be proven here. There are also no `.mxbc`
recordings on this machine, so nothing downstream can be checked against real riding either.
