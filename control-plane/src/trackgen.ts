/**
 * Writing a track program with Claude.
 *
 * The model never sees a heightmap. It writes a short document — a start pose, a run of
 * straights and arcs, and the jumps laid along them — and the app turns that into terrain.
 * So this endpoint is small: a schema, the numbers real tracks measure, and one call.
 *
 * The key lives here rather than in the app, and so do the system prompt and the schema. That
 * is the point of the split: a stolen app token can be spent on generating motocross tracks
 * and on nothing else.
 *
 * The app validates everything that comes back — it synthesises the terrain and measures it
 * against published tracks — and sends the failures to `problems` on the next call. Believing
 * a program because it parsed is the mistake this whole arrangement exists to avoid, so
 * nothing here tries to be the last line of defence.
 */

import Anthropic from "@anthropic-ai/sdk";
import { zodOutputFormat } from "@anthropic-ai/sdk/helpers/zod";
import { z } from "zod";

/**
 * The track program, mirroring `src-tauri/src/trackprog.rs`.
 *
 * These two have to agree, and nothing enforces it but this comment and the app's own parse
 * — which fails loudly, at the boundary, rather than producing a wrong track. Change one,
 * change the other.
 */
const RISE = z
  .number()
  .describe("metres the ground climbs over this segment; negative drops, 0 follows the land");

const Straight = z.object({
  kind: z.literal("straight"),
  length: z.number().describe("metres"),
  rise: RISE,
});

const Arc = z.object({
  kind: z.literal("arc"),
  radius: z
    .number()
    .describe("metres, signed — positive turns right, negative turns left"),
  angle: z.number().describe("degrees swept, always positive"),
  rise: RISE,
});

/**
 * The four features that are the same shape as each other, carried as one variant.
 *
 * Not a simplification of the track program — the wire format is byte-for-byte what eight
 * separate variants produced, because each object still names its own `kind`. It is a
 * constrained-decoding limit: the API compiles this schema into a grammar, and a union of
 * eight object alternatives inside an array compiles to one too large to accept. Measured
 * rather than guessed — eight is rejected and six is accepted, on every model from Haiku 4.5
 * up to Opus 5, so this was never a question of paying for a bigger model.
 *
 * Collapsing the four that share `at`/`length`/`height` into a single variant tagged by an
 * enum takes the union to five and costs nothing. If a fifth same-shaped feature is ever
 * added, it belongs in this enum rather than beside it.
 */
const SimpleFeature = z.object({
  kind: z.enum(["tabletop", "roller", "stepUp", "berm"]),
  at: z.number().describe("metres round the lap from the start"),
  length: z.number(),
  height: z
    .number()
    .describe("metres; for stepUp, the ground gained — negative for a step down"),
});

const Feature = z.discriminatedUnion("kind", [
  SimpleFeature,
  z.object({
    kind: z.literal("double"),
    at: z.number(),
    height: z.number(),
    gap: z.number().describe("metres of flat ground between the two jumps"),
    lip: z.number().describe("metres of takeoff face"),
  }),
  z.object({
    kind: z.literal("whoops"),
    at: z.number(),
    count: z.number().int(),
    spacing: z.number().describe("metres crest to crest"),
    height: z.number(),
  }),
  z.object({
    kind: z.literal("custom"),
    at: z.number(),
    length: z.number(),
    shape: z
      .array(z.object({ u: z.number(), h: z.number() }))
      .describe('heights along the feature; u runs 0 at its start to 1 at its end'),
  }),
  z.object({
    kind: z.literal("rut"),
    at: z.number(),
    length: z.number(),
    depth: z.number().describe("metres; corners grow their own, this is for elsewhere"),
  }),
]);

const TrackProgram = z.object({
  name: z.string(),
  author: z.string(),
  location: z.string(),
  width: z.number().describe("the riding line, metres"),
  terrain: z.object({
    sizeX: z.number().describe("metres of ground across"),
    sizeZ: z.number().describe("metres of ground down"),
    samples: z.number().int().describe("a power of two plus one; 2049 unless there is a reason"),
    scale: z
      .number()
      .describe(
        "the whole height budget in metres — everything is quantised against it, so keep it just above the tallest thing on the track",
      ),
    relief: z.object({
      amplitude: z.number().describe("the landscape's own hills, peak to trough, metres"),
      wavelength: z.number().describe("metres between those hills"),
      seed: z.number().int(),
      texture: z.number().describe("ridden-surface roughness, metres; 0.06 is normal"),
    }),
    surface: z
      .enum(["soil", "sand", "grass"])
      .describe("what the ground is, either side of the riding line"),
  }),
  start: z.object({
    x: z.number(),
    z: z.number(),
    angle: z.number().describe("degrees; 0 looks down +z and increases clockwise towards +x"),
  }),
  blend: z
    .number()
    .describe(
      "metres things ease into each other over: where two jumps meet, where a straight becomes a corner, and how long a jump's own ramps are. 1.2 is normal; 0 is every edge sharp",
    ),
  elevation: z
    .array(
      z.object({
        at: z.number().describe('metres round the lap'),
        height: z.number().describe('metres above the ground the track would otherwise follow'),
      }),
    )
    .describe('the lap height as a curve; leave empty to follow the landscape'),
  segments: z.array(z.discriminatedUnion("kind", [Straight, Arc])),
  features: z.array(Feature),
});

/**
 * What published MX Bikes tracks actually measure, from the app's own survey of installed
 * ones. Quoted rather than described: a model given "13 m wide" writes a track, a model given
 * "realistic" writes a guess.
 */
export const SYSTEM = `You design motocross tracks for MX Bikes as a "track program" — a document the
game's own terrain compiler is built from. You never write a heightmap.

A lap is a start pose plus a list of segments, each either a straight of some length or an arc
of a signed radius through some angle. Positive radius turns right, negative left. Features —
jumps, whoops, berms — are placed by how far round the lap they are, in metres.

IF THE BRIEF DESCRIBES THE LAP IN ORDER, FOLLOW IT LITERALLY. "A long straight into a left
hairpin, then a double and a rhythm section" is a specification, not a mood: emit a straight,
then an arc with a negative radius, then a double, then a run of jumps — in that order, and
with nothing inserted between them that was not asked for. Add segments only where they are
needed to bring the lap back to the start. A brief that describes an atmosphere instead ("a
sandy national") leaves the layout to you.

THE LAP MUST CLOSE. The signed turn angles have to sum to exactly ±360° (or a multiple), and
the straights have to bring it home. A lap that misses itself is the single most common
failure and it is always caught.

Close it by doing the arithmetic. Point symmetry — half a lap whose signed angles sum to
±180°, repeated with every feature offset by the half-lap's length — guarantees closure and is
there as a fallback, but a track built that way is symmetrical about its own centre and reads
as one on the map. Use it only if the layout you want will not close.

WORK ROUND A CENTRE, ONCE. This is how to build a lap that cannot cross itself, and it is a
construction rather than a thing to check afterwards. Picture the infield as a clock face and
go round it exactly once: every segment should leave you further round the clock than the one
before, and you arrive back at twelve where you started. A lap laid out this way is physically
incapable of running over its own ground, because it never comes back to an hour it has
already passed.

Corners are what carry you round the clock; straights are what carry you outward and along.
Counter-turns — turning the other way for character — are allowed and good, but they must be
gentler than the turns either side of them, or you stop advancing round the clock, double back,
and cross. Check as you write: add the signed angles and they must reach ±360 exactly; add the
absolute angles and a clean lap comes to somewhere around 900. Much past that and you have
gone round twice, which means you have crossed.

The other way to cross is a straight that simply overshoots — long enough to reach back across
a part of the lap you drew earlier. Long straights are good, but a straight that would carry
you past the middle of the infield is too long.

WRITE A LAP, NOT A SHAPE. This is the thing generated tracks get most wrong, and it is not a
matter of taste — a track's height file carries the centreline its builder typed, so we can
read exactly
what ten published circuits are made of:

  segments in a lap   52-150. Not twenty.
  arcs vs straights   61-91% ARCS. Indiana is 109 arcs against 11 straights; Millville 132
                      against 18. A circuit is a chain of corners with a few straights let
                      into it, NOT straights joined by corners.
  total turning       1726-2960°, adding up every degree turned either way. A rounded
                      rectangle comes to 900. Nothing published is under 1700.
  corners             13-25 of them, counting a run of same-way arcs as one corner.
  a corner            103-170° at the median, and never one arc. A published corner is three
                      to eighteen arcs whose radius tightens into the apex and releases out
                      of it: 28 m through 30°, then 13 m through 50°, then 9 m through 57°,
                      then 18 m through 30° is ONE corner. Writing it as a single 165° arc of
                      constant radius is the clearest sign a lap was drawn rather than built.
  tightest radius     10.6-18.5 m at the median corner, down to 6.4 m at the hairpins.
  straights           short. Indiana's run 21 m at the median and its longest is 62 m.
  lap length          1800-2500 m.

So: build each corner as a run of arcs, keep the straights short, and let the lap wander.
Count the arcs before you send it — if straights outnumber corners you have written a shape.

THE LAP MUST STILL CLOSE, and a lap like this closes the same way: the signed angles sum to
±360° and the straights bring it home. A serpentine that turns 2400° in total and 360° net is
exactly what the published tracks do — they alternate a big turn one way with a slightly
smaller one back, which advances round the clock face by the difference. A 165° right followed
by a 120° left advances 45°; eight of those pairs is 360° and a lap.

Corners grow their own ruts, braking bumps and berms — do not ask for any of those in a
corner, they are already there and measured off published ground. A rut feature is for putting
one somewhere a corner would not, and a berm feature is for a wall taller than the half-metre
a corner banks itself.

Set terrain.surface from the brief: sand for a sand track, grass for an early-season or
grasstrack circuit, soil for everything else.

Elevation is per segment: rise is metres gained across it, negative for a drop. A lap that
climbs has to come back down, so the rises must sum to about zero or the finish ends up above
the start. Leave them 0 to follow the landscape, which is what most of a track does.

Berms bank the OUTSIDE of a corner, so only place one where an arc segment is — a berm on a
straight does nothing. Jumps go on straights.

What real tracks measure, from a survey of published ones. Land inside these unless the brief
explicitly asks otherwise:

  jumps on a lap      COUNT THEM. A 2000 m lap carries 30–45 features — not four.
  landscape relief    amplitude 8–20 m over a 120–200 m wavelength. A flat plot measures
                      under 18° at its steepest and is rejected for it; ground has to roll.
  riding line width   10–17 m
  jumps per km        12–25, measured along the centrelines of published tracks
  the SIZE MIX        this is what makes a lap read as a national rather than a rhythm
                      section. Published laps carry 3–11 jumps per km over a metre and
                      everything else UNDER one: Indiana has forty features and only fourteen
                      stand over a metre. Six or eight big ones of 2.5–4 m, and the rest
                      rollers of 0.4–0.9 m. A lap of thirty identical 1.5 m tabletops is
                      wrong in both directions at once.
  a jump's height     0.4–0.9 m for a roller, 1.0–2.0 m for an ordinary jump, 2.5–4.0 m for
                      the handful that matter (it measures about 0.75x that against the
                      landscape). Published lips top out at 3.0–5.9 m.
  jump spacing        20–50 m between takeoffs
  whoop spacing       4–6 m crest to crest
  corner radius       7–30 m at the tightest point of a corner; 40 m and up barely turns
  steepest ground     27–41°

Keep the lap inside the terrain with at least a track's width of margin on every side, and set
the height budget (terrain.scale) to roughly twice the landscape amplitude plus the tallest
jump — too small and the build is rejected, far too large and the terrain quantises coarsely.

The app builds and measures whatever you send. If it comes back with problems, they carry the
measured number and what published tracks do; edit the program you sent rather than starting
over.`;

/**
 * The model that writes the lap.
 *
 * The cheapest one that can do the job, on purpose: $1/$5 per MTok against Opus 5's $5/$25.
 * A lap is a few thousand output tokens, so an attempt costs well under a cent, and the app
 * validates every answer by synthesising the terrain and measuring it — a weaker model that
 * needs a second attempt is still far cheaper than a stronger one that gets it first time.
 *
 * The thing to watch is lap closure: the signed turn angles have to sum to ±360°, which is
 * arithmetic rather than judgement, and it is the one part of this a small model is likely to
 * get wrong repeatedly. The validator catches it and says so with the number, but if repairs
 * start costing more than they save, `claude-sonnet-5` ($2/$10) is the next rung up and takes
 * the same request shape as this one.
 */
const MODEL = "claude-haiku-4-5";

/**
 * Which model, and how it wants to be asked.
 *
 * `TRACK_MODEL` overrides the default, because which model this wants is a running cost
 * decision rather than a code one — put it in `.dev.vars` locally or set it as a var on the
 * deployment. The two families take different parameters and the mismatch is a 400 rather
 * than something ignored: Haiku 4.5 predates adaptive thinking, takes a fixed thinking
 * budget, and rejects `output_config.effort` outright.
 *
 * Worth knowing before turning the cheap one on: Haiku writes laps that cross over themselves
 * and cannot reliably fix one when told. It is the one thing here that is genuinely spatial
 * reasoning, there is nothing to compute on its behalf, and it is where the price difference
 * actually shows up.
 */
function ask(model: string) {
  const adaptive = !/^claude-haiku/.test(model);
  return {
    model,
    thinking: adaptive
      ? ({ type: "adaptive" } as const)
      : ({ type: "enabled", budget_tokens: 4000 } as const),
    effort: adaptive ? ("low" as const) : undefined,
  };
}

/** Briefs longer than this are not briefs. */
const MAX_BRIEF = 2000;

/** How much of a rejected program to hand back. A lap is a few thousand tokens. */
const MAX_PREVIOUS = 60_000;

type Body = {
  brief?: unknown;
  previous?: unknown;
  problems?: unknown;
};

export async function generateTrack(request: Request, env: Env): Promise<Response> {
  if (!env.ANTHROPIC_API_KEY) {
    return json(503, { error: "track generation isn't configured on this deployment" });
  }

  let body: Body;
  try {
    body = (await request.json()) as Body;
  } catch {
    return json(400, { error: "expected a JSON body" });
  }

  const brief = typeof body.brief === "string" ? body.brief.trim() : "";
  if (!brief) return json(400, { error: "say what kind of track you want" });
  if (brief.length > MAX_BRIEF) {
    return json(400, { error: `keep the brief under ${MAX_BRIEF} characters` });
  }

  const problems = Array.isArray(body.problems)
    ? body.problems.filter((p): p is string => typeof p === "string").slice(0, 40)
    : [];
  const previous =
    typeof body.previous === "string" ? body.previous.slice(0, MAX_PREVIOUS) : null;

  const messages: Anthropic.MessageParam[] = [{ role: "user", content: brief }];
  if (previous && problems.length) {
    // The model gets its own answer back and a list of measurements, which is a far easier
    // thing to act on than a fresh attempt at the same brief.
    messages.push({ role: "assistant", content: previous });
    messages.push({
      role: "user",
      content: `The app built that and measured it. These are wrong:\n\n${problems
        .map((p) => `- ${p}`)
        .join("\n")}\n\nSend the whole program again with those fixed.`,
    });
  }

  const client = new Anthropic({ apiKey: env.ANTHROPIC_API_KEY });
  try {
    const chosen = ask(env.TRACK_MODEL?.trim() || MODEL);
    const response = await client.messages.parse({
      model: chosen.model,
      max_tokens: 16000,
      system: SYSTEM,
      messages,
      // Laying out a lap is arithmetic the model would have to do — except most of it is
      // done for it now, see `repair` in trackllm.rs.
      thinking: chosen.thinking,
      output_config: {
        format: zodOutputFormat(TrackProgram),
        ...(chosen.effort ? { effort: chosen.effort } : {}),
      },
    });

    if (response.stop_reason === "refusal") {
      return json(422, { error: "the model declined that brief" });
    }
    if (!response.parsed_output) {
      return json(422, { error: "the model's answer didn't fit the track schema" });
    }
    return json(200, { program: response.parsed_output });
  } catch (err) {
    // An answer that doesn't fit the schema is the model's mistake, not the service's, and
    // it is the single most common thing a small model gets wrong here — it invents a
    // feature kind. Reported as a 422 so the app's loop treats it as something to send back
    // and fix; a 500 aborts the whole loop on the first stumble, which threw away two
    // perfectly good remaining attempts.
    if (err instanceof Error && /structured output|parse/i.test(err.message)) {
      return json(422, {
        error: `that didn't fit the track schema: ${err.message.slice(0, 400)}`,
      });
    }
    if (err instanceof Anthropic.RateLimitError) {
      return json(429, { error: "the track service is busy — try again in a minute" });
    }
    if (err instanceof Anthropic.AuthenticationError) {
      return json(503, { error: "track generation isn't configured on this deployment" });
    }
    if (err instanceof Anthropic.APIError) {
      return json(502, { error: `the model service failed: ${err.message}` });
    }
    throw err;
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
