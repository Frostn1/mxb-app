/**
 * Writing a track with Claude.
 *
 * The model never sees a heightmap. It answers in one of two shapes, and the app turns either
 * into terrain, or asks for a constrained edit to the open track/paint document:
 *
 * - a **program**: a start pose, a run of straights and arcs, and the jumps laid along them.
 *   The model draws the whole lap and the app measures it. Only a strong model manages this.
 * - **settings**: the character only (how long, how tight, what ground, how big the jumps).
 *   The app's own lap walker draws the lap from them, so the answer is small and any model can
 *   give it.
 *
 * The key lives here rather than in the app. That is the point of the split: a stolen app
 * token can be spent on generating motocross tracks and on nothing else. The prompts and the
 * schemas live in packages/track-protocol, because the Studio reads the same files to ask a
 * model of the user's own directly.
 *
 * The app validates everything that comes back — it synthesises the terrain and measures it
 * against published tracks — and sends the failures to `problems` on the next call. Believing
 * a program because it parsed is the mistake this whole arrangement exists to avoid, so
 * nothing here tries to be the last line of defence.
 */

import Anthropic from "@anthropic-ai/sdk";
import { jsonSchemaOutputFormat } from "@anthropic-ai/sdk/helpers/json-schema";

import PROGRAM_SCHEMA from "../../packages/track-protocol/program.schema.json";
import PROGRAM_SYSTEM from "../../packages/track-protocol/program.system.md";
import SETTINGS_SCHEMA from "../../packages/track-protocol/settings.schema.json";
import SETTINGS_SYSTEM from "../../packages/track-protocol/settings.system.md";
import EDIT_SYSTEM from "../../packages/track-protocol/edit.system.md";
import PAINT_EDIT_SCHEMA from "../../packages/track-protocol/paint-edit.schema.json";
import PAINT_EDIT_SYSTEM from "../../packages/track-protocol/paint-edit.system.md";

/**
 * The whole-lap prompt: what published MX Bikes tracks measure, quoted rather than described.
 * A model given "13 m wide" writes a track; a model given "realistic" writes a guess.
 */
export const SYSTEM = PROGRAM_SYSTEM;

type Schema = Parameters<typeof jsonSchemaOutputFormat>[0];

/**
 * What the app can ask for, and the key its answer comes back under.
 *
 * The schemas carry no optional fields and one union between them. The API compiles a schema
 * into a grammar with a ceiling on its size, and what costs is alternatives: a `nullable`
 * field is one too. So every field is required and a kind that does not use one writes 0. The
 * tests pin the union count.
 */
export const PROTOCOLS = {
  program: {
    system: PROGRAM_SYSTEM,
    schema: PROGRAM_SCHEMA as unknown as Schema,
    // A published-length lap is 130 segments and forty features, and the thinking that lays
    // it out counts against the same ceiling. At 16000 the two ran off the end and every
    // attempt came back "that didn't parse". Not raised further because the app gives up at
    // ten minutes.
    maxTokens: 32000,
    think: true,
  },
  settings: {
    system: SETTINGS_SYSTEM,
    schema: SETTINGS_SCHEMA as unknown as Schema,
    // Twenty fields, one of which is which discipline the brief asks for. There is no
    // arithmetic in it for thinking to help with.
    maxTokens: 2000,
    think: false,
  },
  trackEdit: {
    system: EDIT_SYSTEM,
    schema: PROGRAM_SCHEMA as unknown as Schema,
    maxTokens: 32000,
    think: true,
  },
  paintEdit: {
    system: PAINT_EDIT_SYSTEM,
    schema: PAINT_EDIT_SCHEMA as unknown as Schema,
    maxTokens: 3000,
    think: false,
  },
} as const;

export type Mode = keyof typeof PROTOCOLS;

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
/** An edit carries the open document and measured UV regions with it. */
const MAX_EDIT_CONTEXT = 100_000;

/** How much of a rejected program to hand back. A lap is a few thousand tokens. */
const MAX_PREVIOUS = 60_000;

type Body = {
  brief?: unknown;
  /** "program" (the default, and what older apps send by leaving it out) or "settings". */
  mode?: unknown;
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

  if (
    body.mode !== undefined &&
    body.mode !== "program" &&
    body.mode !== "settings" &&
    body.mode !== "trackEdit" &&
    body.mode !== "paintEdit"
  ) {
    return json(400, { error: "mode is program, settings, trackEdit or paintEdit" });
  }
  const mode: Mode = body.mode ?? "program";
  const protocol = PROTOCOLS[mode];
  const brief = typeof body.brief === "string" ? body.brief.trim() : "";
  if (!brief) return json(400, { error: "say what you want" });
  const inputLimit = mode === "program" || mode === "settings" ? MAX_BRIEF : MAX_EDIT_CONTEXT;
  if (brief.length > inputLimit) {
    return json(400, { error: `keep the request under ${inputLimit} characters` });
  }

  const problems = Array.isArray(body.problems)
    ? body.problems.filter((p): p is string => typeof p === "string").slice(0, 40)
    : [];
  const previous =
    typeof body.previous === "string" ? body.previous.slice(0, MAX_PREVIOUS) : null;

  const messages: Anthropic.MessageParam[] = [{ role: "user", content: brief }];
  // Settings are always legal once clamped, so the app never sends them back to be fixed.
  if ((mode === "program" || mode === "trackEdit") && previous && problems.length) {
    // The model gets its own answer back and a list of measurements, which is a far easier
    // thing to act on than a fresh attempt at the same brief.
    messages.push({ role: "assistant", content: previous });
    messages.push({
      role: "user",
      content: `The app built that and measured it. These are wrong:\n\n${problems
        .map((p) => `- ${p}`)
        .join("\n")}\n\nSend the whole program again with those fixed.`,
    });
  } else if (mode === "paintEdit" && problems.length) {
    if (previous) messages.push({ role: "assistant", content: previous });
    messages.push({
      role: "user",
      content: `That action plan was rejected: ${problems.join(
        "; ",
      )}. Send a corrected plan for the original request.`,
    });
  }

  const client = new Anthropic({ apiKey: env.ANTHROPIC_API_KEY });
  try {
    const chosen = ask(env.TRACK_MODEL?.trim() || MODEL);
    // Streamed, and not for the progress: the SDK refuses a non-streaming request whose
    // ceiling could take it past ten minutes, so `max_tokens` cannot be raised without this.
    // The answer is still assembled here and returned whole — the app waits for one JSON body.
    const stream = client.messages.stream({
      model: chosen.model,
      max_tokens: protocol.maxTokens,
      system: protocol.system,
      messages,
      // Laying out a lap is arithmetic the model would have to do — except most of it is
      // done for it now, see `repair` in trackllm.rs.
      ...(protocol.think ? { thinking: chosen.thinking } : {}),
      output_config: {
        format: jsonSchemaOutputFormat(protocol.schema),
        ...(chosen.effort ? { effort: chosen.effort } : {}),
      },
    });
    const response = await stream.finalMessage();

    if (response.stop_reason === "refusal") {
      return json(422, { error: "the model declined that brief" });
    }
    if (!response.parsed_output) {
      return json(422, { error: "the model's answer didn't fit the track schema" });
    }
    return json(200, { [mode]: response.parsed_output });
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
