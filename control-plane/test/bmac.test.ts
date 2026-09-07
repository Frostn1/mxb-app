import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { announcement, bmacWebhook, signatureMatches } from "../src/bmac";

const SECRET = "whsec_test";
const CHANNEL = "https://discord.test/webhook";

/** The signature BMAC would send for this body. */
async function sign(raw: string, secret = SECRET): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(raw));
  return [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** A D1 stand-in that remembers which event ids have been claimed. */
function stubDb(seen = new Set<string>()) {
  return {
    seen,
    prepare(sql: string) {
      return {
        bind(...args: unknown[]) {
          const id = String(args[0]);
          return {
            async first() {
              if (!sql.startsWith("INSERT")) return null;
              if (seen.has(id)) return null; // ON CONFLICT DO NOTHING returns no row
              seen.add(id);
              return { event_id: id };
            },
            async run() {
              if (sql.startsWith("DELETE")) seen.delete(id);
              return { success: true };
            },
          };
        },
      };
    },
  };
}

function stubEnv(overrides: Record<string, unknown> = {}): Env {
  return {
    BMAC_WEBHOOK_SECRET: SECRET,
    DISCORD_DONATION_WEBHOOK_URL: CHANNEL,
    DB: stubDb(),
    ...overrides,
  } as unknown as Env;
}

async function deliver(body: unknown, env: Env, signature?: string): Promise<Response> {
  const raw = JSON.stringify(body);
  const headers: Record<string, string> = { "content-type": "application/json" };
  const sig = signature ?? (await sign(raw));
  if (sig) headers["x-signature-sha256"] = sig;
  return bmacWebhook(new Request("https://cp.test/v1/bmac/webhook", { method: "POST", body: raw, headers }), env);
}

const COFFEE = {
  event_id: 4242,
  type: "donation.created",
  live_mode: true,
  created: 1_770_000_000,
  data: {
    supporter_name: "Mouk",
    support_note: "love the app",
    supporter_email: "mouk@example.com",
    amount: 15,
    currency: "USD",
    coffee_count: 3,
  },
};

let posts: { url: string; body: any }[] = [];
let discordStatus = 204;

beforeEach(() => {
  posts = [];
  discordStatus = 204;
  vi.stubGlobal("fetch", async (url: unknown, init: RequestInit) => {
    posts.push({ url: String(url), body: JSON.parse(String(init.body)) });
    return new Response(null, { status: discordStatus });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("signature", () => {
  it("accepts the signature BMAC would send", async () => {
    const raw = JSON.stringify(COFFEE);
    expect(await signatureMatches(raw, SECRET, await sign(raw))).toBe(true);
    // Sent uppercase, or with the algorithm prefixed, is still the same digest.
    expect(await signatureMatches(raw, SECRET, (await sign(raw)).toUpperCase())).toBe(true);
    expect(await signatureMatches(raw, SECRET, `sha256=${await sign(raw)}`)).toBe(true);
  });

  it("rejects a body that was changed after signing", async () => {
    const signature = await sign(JSON.stringify(COFFEE));
    const res = await deliver({ ...COFFEE, data: { supporter_name: "Impostor" } }, stubEnv(), signature);
    expect(res.status).toBe(401);
    expect(posts).toHaveLength(0);
  });

  it("rejects a body signed with the wrong secret, and one with no signature at all", async () => {
    const raw = JSON.stringify(COFFEE);
    expect(await signatureMatches(raw, SECRET, await sign(raw, "not-the-secret"))).toBe(false);
    expect(await signatureMatches(raw, SECRET, null)).toBe(false);
    expect((await deliver(COFFEE, stubEnv(), "")).status).toBe(401);
  });

  it("answers 503 where the deployment holds no secret, and never posts", async () => {
    const res = await deliver(COFFEE, stubEnv({ BMAC_WEBHOOK_SECRET: undefined }));
    expect(res.status).toBe(503);
    expect((await deliver(COFFEE, stubEnv({ DISCORD_DONATION_WEBHOOK_URL: undefined }))).status).toBe(503);
    expect(posts).toHaveLength(0);
  });
});

describe("what reaches the channel", () => {
  it("announces a coffee by name, with the note", async () => {
    const res = await deliver(COFFEE, stubEnv());
    expect(res.status).toBe(200);
    expect(posts).toHaveLength(1);
    expect(posts[0].url).toBe(CHANNEL);
    const embed = posts[0].body.embeds[0];
    expect(embed.description).toContain("Mouk");
    expect(embed.description).toContain("bought you a coffee");
    expect(embed.fields[0].value).toContain("love the app");
  });

  it("never carries the supporter's email or what they paid", async () => {
    await deliver(COFFEE, stubEnv());
    const sent = JSON.stringify(posts[0].body);
    expect(sent).not.toContain("mouk@example.com");
    expect(sent).not.toContain("example.com");
    expect(sent).not.toMatch(/\$|USD|\b15\b/);
    // The coffee count is money in disguise — three coffees is a price.
    expect(sent).not.toContain("coffee_count");
    expect(sent).not.toMatch(/\b3 coffees\b/);
  });

  it("names the tier on a membership, since that is what decides their group", async () => {
    const payload: any = announcement({
      event_id: 1,
      type: "membership.started",
      live_mode: true,
      data: { supporter_name: "Kelso", membership_level_name: "Pit Crew" },
    });
    expect(payload.embeds[0].description).toContain("Kelso");
    expect(payload.embeds[0].description).toContain("Pit Crew");
  });

  it("marks a test delivery as one", async () => {
    const payload: any = announcement({ ...COFFEE, live_mode: false });
    expect(payload.embeds[0].description).toContain("test event");
  });

  it("still announces an event whose fields it doesn't recognise", async () => {
    const payload: any = announcement({ event_id: 9, type: "donation.created", data: {} });
    expect(payload.embeds[0].description).toContain("Someone");
    expect(payload.embeds[0].fields).toBeUndefined();
  });

  it("won't let a note ping the server or restyle the embed", async () => {
    const payload: any = announcement({
      event_id: 2,
      type: "donation.created",
      data: { supporter_name: "@everyone", support_note: "@everyone **hi** `x`" },
    });
    // `@everyone` survives as text — it is what they wrote — but allowed_mentions means it
    // pings nobody, and every markdown character comes through escaped.
    expect(payload.allowed_mentions).toEqual({ parse: [] });
    expect(payload.embeds[0].fields[0].value).toBe("@everyone \\*\\*hi\\*\\* \\`x\\`");
  });

  it("clips a very long note", async () => {
    const payload: any = announcement({
      event_id: 3,
      type: "donation.created",
      data: { supporter_name: "Long", support_note: "a".repeat(2000) },
    });
    expect(payload.embeds[0].fields[0].value.length).toBeLessThanOrEqual(400);
  });
});

describe("what doesn't", () => {
  it("accepts a refund, a cancellation and an unknown type without posting", async () => {
    const env = stubEnv();
    for (const type of [
      "donation.refunded",
      "membership.cancelled",
      "membership.paused",
      "recurring_donation.updated",
      "something.new",
    ]) {
      const res = await deliver({ ...COFFEE, event_id: `${type}-1`, type }, env);
      // 200, not an error: a non-2xx would only make BMAC retry an event we mean to drop.
      expect(res.status).toBe(200);
      expect(announcement({ type })).toBeNull();
    }
    expect(posts).toHaveLength(0);
  });

  it("announces a redelivered event exactly once", async () => {
    const env = stubEnv();
    expect((await deliver(COFFEE, env)).status).toBe(200);
    expect((await deliver(COFFEE, env)).status).toBe(200);
    expect((await deliver(COFFEE, env)).status).toBe(200);
    expect(posts).toHaveLength(1);
  });

  it("lets the retry through when Discord refused the post", async () => {
    const env = stubEnv();
    discordStatus = 500;
    expect((await deliver(COFFEE, env)).status).toBe(502);
    discordStatus = 204;
    expect((await deliver(COFFEE, env)).status).toBe(200);
    expect(posts).toHaveLength(2); // the failed attempt, then the one that landed
  });
});
