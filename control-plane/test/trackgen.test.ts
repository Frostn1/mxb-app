import { describe, expect, it } from "vitest";

import { PROTOCOLS, SYSTEM, generateTrack } from "../src/trackgen";

/** A request the endpoint would accept, so each test can spoil one thing about it. */
function post(body: unknown): Request {
  return new Request("https://example.invalid/v1/track/generate", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}

const withKey = { ANTHROPIC_API_KEY: "sk-test" } as unknown as Env;
const withoutKey = {} as unknown as Env;

describe("what it refuses before spending anything", () => {
  it("says so when the deployment has no key, rather than failing on the call", async () => {
    const res = await generateTrack(post({ brief: "a sandy national" }), withoutKey);
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({
      error: "track generation isn't configured on this deployment",
    });
  });

  it("rejects a body that isn't JSON", async () => {
    const res = await generateTrack(post("not json at all"), withKey);
    expect(res.status).toBe(400);
  });

  it("rejects an empty brief", async () => {
    for (const brief of [undefined, "", "   ", 42]) {
      const res = await generateTrack(post({ brief }), withKey);
      expect(res.status, `brief=${JSON.stringify(brief)}`).toBe(400);
    }
  });

  it("caps the brief, so the endpoint can't be used as a general-purpose prompt", async () => {
    const res = await generateTrack(post({ brief: "x".repeat(2001) }), withKey);
    expect(res.status).toBe(400);
    expect(await res.json()).toMatchObject({ error: expect.stringContaining("2000") });
  });
});

describe("the no-key check comes first", () => {
  // Every one of these would otherwise reach the API. A deployment without a key must never
  // get that far, whatever it is sent.
  it("answers 503 for a valid brief with repair feedback attached", async () => {
    const res = await generateTrack(
      post({
        brief: "a sandy national",
        previous: '{"name":"x"}',
        problems: ["the riding line is 31.0 m; published tracks run 8–20 m"],
      }),
      withoutKey,
    );
    expect(res.status).toBe(503);
  });
});

describe("settings mode", () => {
  it("answers 503 without a key too", async () => {
    const res = await generateTrack(post({ brief: "a sandy national", mode: "settings" }), withoutKey);
    expect(res.status).toBe(503);
  });

  it("refuses a mode it doesn't know", async () => {
    const res = await generateTrack(post({ brief: "a sandy national", mode: "heightmap" }), withKey);
    expect(res.status).toBe(400);
  });
});

describe("the prompts", () => {
  it("load as text from packages/track-protocol", () => {
    expect(typeof SYSTEM).toBe("string");
    expect(SYSTEM.length).toBeGreaterThan(1000);
    expect(SYSTEM).toContain("THE LAP MUST CLOSE");
    expect(PROTOCOLS.settings.system).toContain("settings");
  });
});

/** Every object node in a schema, so each can be checked for strict mode. */
function objects(node: unknown, out: Record<string, unknown>[] = []): Record<string, unknown>[] {
  if (Array.isArray(node)) node.forEach((n) => objects(n, out));
  else if (node && typeof node === "object") {
    const o = node as Record<string, unknown>;
    if (o.type === "object") out.push(o);
    Object.values(o).forEach((n) => objects(n, out));
  }
  return out;
}

describe("the schemas", () => {
  it("stay under the grammar ceiling", () => {
    // Alternatives are what cost, and a nullable field is one. Measured 2026-09-06: the
    // program fits with the segment union and nothing else. Raise this only after checking
    // the live API still compiles it.
    const unions = (s: unknown) => JSON.stringify(s).split('"anyOf"').length - 1;
    expect(unions(PROTOCOLS.program.schema)).toBeLessThanOrEqual(1);
    expect(unions(PROTOCOLS.settings.schema)).toBe(0);
  });

  it("are strict: every field required and nothing extra", () => {
    // What Groq, OpenAI and Anthropic all need for constrained decoding.
    for (const mode of ["program", "settings"] as const) {
      for (const o of objects(PROTOCOLS[mode].schema)) {
        const keys = Object.keys(o.properties as object).sort();
        expect([...(o.required as string[])].sort(), `${mode}: ${keys}`).toEqual(keys);
        expect(o.additionalProperties, `${mode}: ${keys}`).toBe(false);
      }
    }
  });
});
