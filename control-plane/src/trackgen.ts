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

const Feature = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("tabletop"),
    at: z.number().describe("metres round the lap from the start"),
    length: z.number(),
    height: z.number(),
  }),
  z.object({
    kind: z.literal("double"),
    at: z.number(),
    height: z.number(),
    gap: z.number().describe("metres of flat ground between the two jumps"),
    lip: z.number().describe("metres of takeoff face"),
  }),
  z.object({
    kind: z.literal("roller"),
    at: z.number(),
    length: z.number(),
    height: z.number(),
  }),
  z.object({
    kind: z.literal("whoops"),
    at: z.number(),
    count: z.number().int(),
    spacing: z.number().describe("metres crest to crest"),
    height: z.number(),
  }),
  z.object({
    kind: z.literal("stepUp"),
    at: z.number(),
    length: z.number(),
    height: z.number().describe("metres the ground gains; negative for a step down"),
  }),
  z.object({
    kind: z.literal("berm"),
    at: z.number(),
    length: z.number(),
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
const SYSTEM = `You design motocross tracks for MX Bikes as a "track program" — a document the
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
the straights have to bring it home. The reliable way to guarantee this is point symmetry:
write half a lap whose signed angles sum to ±180°, then repeat that same half with every
feature offset by the half-lap's length. A lap that misses itself is the single most common
failure and it is always caught.

Corners grow their own ruts and braking bumps — do not ask for a rut in a corner, it is
already there. A rut feature is for putting one somewhere a corner would not.

Set terrain.surface from the brief: sand for a sand track, grass for an early-season or
grasstrack circuit, soil for everything else.

Elevation is per segment: rise is metres gained across it, negative for a drop. A lap that
climbs has to come back down, so the rises must sum to about zero or the finish ends up above
the start. Leave them 0 to follow the landscape, which is what most of a track does.

Berms bank the OUTSIDE of a corner, so only place one where an arc segment is — a berm on a
straight does nothing. Jumps go on straights.

What real tracks measure, from a survey of published ones. Land inside these unless the brief
explicitly asks otherwise:

  riding line width   10–17 m
  lap length          1300–1800 m
  jumps per km        29–61
  a jump's height     1.2–2.2 m (it measures about 0.75x that against the landscape)
  jump spacing        13–20 m between takeoffs
  whoop spacing       4–6 m crest to crest
  corner radius       12–60 m; tighter than 12 is a hairpin, over 100 is barely a corner
  steepest ground     27–41°

Keep the lap inside the terrain with at least a track's width of margin on every side, and set
the height budget (terrain.scale) to roughly twice the landscape amplitude plus the tallest
jump — too small and the build is rejected, far too large and the terrain quantises coarsely.

The app builds and measures whatever you send. If it comes back with problems, they carry the
measured number and what published tracks do; edit the program you sent rather than starting
over.`;

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
    const response = await client.messages.parse({
      model: "claude-opus-5",
      max_tokens: 16000,
      system: SYSTEM,
      messages,
      // Laying out a lap that closes is arithmetic the model has to actually do.
      thinking: { type: "adaptive" },
      output_config: {
        effort: "high",
        format: zodOutputFormat(TrackProgram),
      },
    });

    if (response.stop_reason === "refusal") {
      return json(422, { error: "the model declined that brief" });
    }
    if (!response.parsed_output) {
      return json(502, { error: "the model's answer didn't fit the track schema" });
    }
    return json(200, { program: response.parsed_output });
  } catch (err) {
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
