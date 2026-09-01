import { describe, expect, it } from "vitest";

import { SYSTEM, generateTrack } from "../src/trackgen";

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

describe("the system prompt", () => {
  it("is one string, not a tagged template", () => {
    // It is a backtick literal, so a stray backtick in the prose ends it and turns the next
    // line into a tag call — `a \`.trh\` carries` parses fine and throws `.trh is not a
    // function` the moment the module is evaluated. It shipped that way once: `tsc` catches
    // it, and `esbuild` and a bundle check both do not.
    expect(typeof SYSTEM).toBe("string");
    expect(SYSTEM.length).toBeGreaterThan(1000);
    expect(SYSTEM).not.toContain("`");
  });
});
