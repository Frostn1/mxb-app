import { describe, expect, it, vi } from "vitest";
import { unwrapContentKey } from "../src/assetkey";
import { adminAssets, parseSteamInput } from "../src/assets";
import { hashToken } from "../src/auth";
import { sealToken, SESSION_COOKIE } from "../src/websession";
import { addAccount, d1 } from "./d1sqlite";

// The entry module exports the voice Durable Object, whose base class only exists in workerd.
vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const ADMIN = "s3cret";
const OWNER = "acc_owner";
const BUYER = "76561198000000042";
const OTHER = "76561198000000043";
const SITE = "https://mxbsecure.com";

function masterKey(): string {
  let s = "";
  for (const b of crypto.getRandomValues(new Uint8Array(32))) s += String.fromCharCode(b);
  return btoa(s);
}

function unb64(text: string): Uint8Array {
  return Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
}

async function deployment(overrides: Partial<Record<string, string | undefined>> = {}) {
  const DB = d1();
  await addAccount(DB, OWNER, "Owner");
  const env = {
    DB,
    ADMIN_KEY: ADMIN,
    MXB_ASSET_MASTER_KEY: masterKey(),
    MXB_OWNER_ACCOUNT_ID: OWNER,
    ...overrides,
  } as unknown as Env;
  return env;
}

function req(
  method: string,
  path: string,
  opts: { body?: unknown; key?: string | null; origin?: string; headers?: Record<string, string> } = {},
): Request {
  const headers: Record<string, string> = { ...opts.headers };
  if (opts.key !== null) headers.Authorization = `Bearer ${opts.key ?? ADMIN}`;
  if (opts.origin) headers.Origin = opts.origin;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";
  return new Request(`https://cp.test${path}`, {
    method,
    headers,
    body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
  });
}

/** Through the real router, the way the site reaches it. */
async function call(env: Env, request: Request): Promise<Response> {
  return worker.fetch(request, env, {} as ExecutionContext);
}

/** Straight at the handler, with Steam stubbed. */
async function direct(env: Env, request: Request, steam: typeof fetch): Promise<Response> {
  return adminAssets(request, new URL(request.url), env, steam);
}

async function create(env: Env, title = "Pro Circuit Livery") {
  const res = await call(env, req("POST", "/admin/assets", { body: { title } }));
  expect(res.status).toBe(201);
  return (await res.json()) as { assetId: string; keyId: string; key: string; title: string };
}

/** Steam's profile XML for known names, its error page for the rest. */
const steamStub = vi.fn(async (input: RequestInfo | URL) => {
  const url = String(input);
  if (url.includes("/profiles/")) return new Response("<profile><steamID><![CDATA[Buyer One]]></steamID></profile>");
  const name = /\/id\/([^/]+)\/\?xml=1$/.exec(url)?.[1];
  if (name === "frostn1") {
    return new Response(`<?xml version="1.0"?><profile><steamID64>${BUYER}</steamID64></profile>`);
  }
  if (name === "down") throw new Error("network");
  return new Response("<response><error>The specified profile could not be found.</error></response>");
}) as unknown as typeof fetch;

describe("POST /admin/assets", () => {
  it("creates an asset whose wrapped key unwraps to the key it returned", async () => {
    const env = await deployment();
    const created = await create(env, "  Pro Circuit Livery  ");
    expect(created.assetId).toMatch(/^ast_[A-Za-z0-9_-]{22}$/);
    expect(created.keyId).toBe("k1");
    expect(created.title).toBe("Pro Circuit Livery");
    const key = unb64(created.key);
    expect(key.length).toBe(32);

    const row = await env.DB.prepare("SELECT * FROM assets WHERE id = ?")
      .bind(created.assetId)
      .first<Record<string, unknown>>();
    expect(row).toMatchObject({
      creator_id: OWNER,
      title: "Pro Circuit Livery",
      key_id: "k1",
      blob_key: null,
      withdrawn_at: null,
    });
    expect(typeof row!.created_at).toBe("number");
    expect(row!.wrapped_key).not.toBe(created.key);
    const unwrapped = await unwrapContentKey(row!.wrapped_key as string, env);
    expect(unwrapped).toEqual(key);
  });

  it("refuses a missing or empty title", async () => {
    const env = await deployment();
    expect((await call(env, req("POST", "/admin/assets", { body: {} }))).status).toBe(400);
    expect((await call(env, req("POST", "/admin/assets", { body: { title: " " } }))).status).toBe(400);
  });

  it("503s when the owner account isn't in the database", async () => {
    const env = await deployment({ MXB_OWNER_ACCOUNT_ID: "acc_missing" });
    const res = await call(env, req("POST", "/admin/assets", { body: { title: "x" } }));
    expect(res.status).toBe(503);
  });
});

describe("auth", () => {
  it("401s without the key or with the wrong one", async () => {
    const env = await deployment();
    expect((await call(env, req("GET", "/admin/assets", { key: null }))).status).toBe(401);
    expect((await call(env, req("GET", "/admin/assets", { key: "guess" }))).status).toBe(401);
    expect((await call(env, req("POST", "/admin/assets", { key: "guess", body: { title: "x" } }))).status).toBe(401);
  });

  it("takes the site's own key here, and nowhere else in /admin", async () => {
    const env = await deployment({ MXB_ASSETS_KEY: "site-key" });
    expect((await call(env, req("GET", "/admin/assets", { key: "site-key" }))).status).toBe(200);
    expect((await call(env, req("GET", "/admin/assets"))).status).toBe(200);
    expect((await call(env, req("GET", "/v1/usage/stats", { key: "site-key" }))).status).not.toBe(200);
    expect((await call(env, req("GET", "/admin/plugins", { key: "site-key" }))).status).not.toBe(200);

    const siteOnly = await deployment({ ADMIN_KEY: undefined, MXB_ASSETS_KEY: "site-key" });
    expect((await call(siteOnly, req("GET", "/admin/assets", { key: "site-key" }))).status).toBe(200);
    expect((await call(siteOnly, req("GET", "/admin/assets", { key: "guess" }))).status).toBe(401);
  });

  it("takes a key from curl with no Origin and no JSON type, writes included", async () => {
    const env = await deployment();
    const { assetId } = await create(env);
    const res = await call(
      env,
      new Request(`https://cp.test/admin/assets/${assetId}/grants`, {
        method: "POST",
        headers: { Authorization: `Bearer ${ADMIN}`, "Content-Type": "text/plain" },
        body: JSON.stringify({ add: [BUYER] }),
      }),
    );
    expect(res.status).toBe(200);
  });

  it("refuses a body over 64 KB with a 413", async () => {
    const env = await deployment();
    const big = await call(env, req("POST", "/admin/assets", { body: { title: "x".repeat(70 * 1024) } }));
    expect(big.status).toBe(413);
    expect(await env.DB.prepare("SELECT COUNT(*) AS n FROM assets").first()).toEqual({ n: 0 });
    // Just under the cap is read (and refused on its own terms).
    const under = await call(env, req("POST", "/admin/assets", { body: { title: "x".repeat(60 * 1024) } }));
    expect(under.status).toBe(400);
  });

  it("503s when the admin key, master key or owner is unset", async () => {
    for (const unset of ["ADMIN_KEY", "MXB_ASSET_MASTER_KEY", "MXB_OWNER_ACCOUNT_ID"]) {
      const env = await deployment({ [unset]: undefined });
      const res = await call(env, req("GET", "/admin/assets"));
      expect(res.status, unset).toBe(503);
    }
  });
});

describe("CORS", () => {
  const preflight = (env: Env, origin: string) =>
    call(
      env,
      req("OPTIONS", "/admin/assets/ast_x/grants", {
        key: null,
        origin,
        headers: { "Access-Control-Request-Method": "POST" },
      }),
    );

  it("answers an allowed preflight 204 before auth", async () => {
    const env = await deployment();
    for (const origin of [SITE, "https://www.mxbsecure.com"]) {
      const res = await preflight(env, origin);
      expect(res.status).toBe(204);
      expect(res.headers.get("Access-Control-Allow-Origin")).toBe(origin);
      expect(res.headers.get("Access-Control-Allow-Methods")).toBe("GET, POST, PATCH, OPTIONS");
      expect(res.headers.get("Access-Control-Allow-Headers")).toBe("Authorization, Content-Type");
      expect(res.headers.get("Vary")).toContain("Origin");
    }
  });

  it("allows a local build of the site only with MXB_ALLOW_DEV_ORIGINS=1", async () => {
    for (const origin of ["http://localhost:5173", "http://127.0.0.1:5173"]) {
      const prod = await preflight(await deployment(), origin);
      expect(prod.status).toBe(403);
      expect(prod.headers.get("Access-Control-Allow-Origin")).toBeNull();
      const dev = await preflight(await deployment({ MXB_ALLOW_DEV_ORIGINS: "1" }), origin);
      expect(dev.status).toBe(204);
      expect(dev.headers.get("Access-Control-Allow-Origin")).toBe(origin);
    }
  });

  it("puts the origin on real responses, errors included", async () => {
    const env = await deployment();
    const ok = await call(env, req("GET", "/admin/assets", { origin: SITE }));
    expect(ok.status).toBe(200);
    expect(ok.headers.get("Access-Control-Allow-Origin")).toBe(SITE);
    const denied = await call(env, req("GET", "/admin/assets", { origin: SITE, key: "guess" }));
    expect(denied.status).toBe(401);
    expect(denied.headers.get("Access-Control-Allow-Origin")).toBe(SITE);
  });

  it("gives a disallowed origin no CORS headers", async () => {
    const env = await deployment();
    const pre = await call(env, req("OPTIONS", "/admin/assets", { key: null, origin: "https://evil.test" }));
    expect(pre.status).toBe(403);
    expect(pre.headers.get("Access-Control-Allow-Origin")).toBeNull();
    const res = await call(env, req("GET", "/admin/assets", { origin: "https://evil.test" }));
    expect(res.headers.get("Access-Control-Allow-Origin")).toBeNull();
  });

  it("stays off every other route", async () => {
    const env = await deployment();
    const res = await call(env, req("OPTIONS", "/admin/plugins", { key: null, origin: SITE }));
    expect(res.headers.get("Access-Control-Allow-Origin")).toBeNull();
    const health = await call(env, req("GET", "/health", { key: null, origin: SITE }));
    expect(health.headers.get("Access-Control-Allow-Origin")).toBeNull();
  });
});

describe("grants", () => {
  it("adds, lists, removes, and re-adds idempotently", async () => {
    const env = await deployment();
    const { assetId } = await create(env);
    const path = `/admin/assets/${assetId}/grants`;

    const add = await call(env, req("POST", path, { body: { add: [BUYER] } }));
    expect(add.status).toBe(200);
    expect(await add.json()).toEqual({
      added: [{ input: BUYER, steamId: BUYER }],
      removed: [],
      failed: [],
    });

    const first = (await (await call(env, req("GET", path))).json()) as {
      grants: { steamId: string; source: string; grantedAt: number; revokedAt: number | null }[];
    };
    expect(first.grants).toHaveLength(1);
    expect(first.grants[0]).toMatchObject({ steamId: BUYER, source: "grant", revokedAt: null });

    // Re-adding an active grant changes nothing.
    await call(env, req("POST", path, { body: { add: [BUYER] } }));
    const again = (await (await call(env, req("GET", path))).json()) as typeof first;
    expect(again.grants).toEqual(first.grants);

    const list = (await (await call(env, req("GET", "/admin/assets"))).json()) as {
      assets: { assetId: string; buyers: number }[];
    };
    expect(list.assets.find((a) => a.assetId === assetId)?.buyers).toBe(1);

    const rm = await call(env, req("POST", path, { body: { remove: [BUYER] } }));
    expect(await rm.json()).toEqual({
      added: [],
      removed: [{ input: BUYER, steamId: BUYER }],
      failed: [],
    });
    const revoked = (await (await call(env, req("GET", path))).json()) as typeof first;
    expect(revoked.grants[0].revokedAt).toBeTypeOf("number");

    // Re-adding lifts the revocation on the same row.
    await call(env, req("POST", path, { body: { add: [BUYER] } }));
    const back = (await (await call(env, req("GET", path))).json()) as typeof first;
    expect(back.grants).toHaveLength(1);
    expect(back.grants[0].revokedAt).toBeNull();
  });

  it("lists assets newest first with only active buyers counted", async () => {
    const env = await deployment();
    const a = await create(env, "First");
    await new Promise((r) => setTimeout(r, 5));
    const b = await create(env, "Second");
    await call(env, req("POST", `/admin/assets/${a.assetId}/grants`, { body: { add: [BUYER, OTHER] } }));
    await call(env, req("POST", `/admin/assets/${a.assetId}/grants`, { body: { remove: [OTHER] } }));
    const body = (await (await call(env, req("GET", "/admin/assets"))).json()) as {
      assets: { assetId: string; title: string; createdAt: number; withdrawnAt: null; buyers: number }[];
    };
    expect(body.assets.map((x) => x.assetId)).toEqual([b.assetId, a.assetId]);
    expect(body.assets[1]).toMatchObject({ title: "First", withdrawnAt: null, buyers: 1 });
    expect(body.assets[0].buyers).toBe(0);
  });

  it("reads every input form, custom URL names through Steam", async () => {
    const env = await deployment();
    const { assetId } = await create(env);
    const inputs = [
      BUYER,
      `https://steamcommunity.com/profiles/${OTHER}/`,
      "steamcommunity.com/id/frostn1",
      "https://steamcommunity.com/id/frostn1/?l=english",
      "FrostN1",
      "https://steamcommunity.com/id/nobody_here",
      "down",
      "not a steam account!",
      "https://steamcommunity.com/profiles/123",
    ];
    const res = await direct(env, req("POST", `/admin/assets/${assetId}/grants`, { body: { add: inputs } }), steamStub);
    expect(res.status).toBe(200);
    const body = (await res.json()) as {
      added: { input: string; steamId: string }[];
      failed: { input: string; error: string }[];
    };
    expect(body.added).toEqual([
      { input: BUYER, steamId: BUYER },
      { input: `https://steamcommunity.com/profiles/${OTHER}/`, steamId: OTHER },
      { input: "steamcommunity.com/id/frostn1", steamId: BUYER },
      { input: "https://steamcommunity.com/id/frostn1/?l=english", steamId: BUYER },
      { input: "FrostN1", steamId: BUYER },
    ]);
    expect(body.failed.map((f) => f.input)).toEqual([
      "https://steamcommunity.com/id/nobody_here",
      "down",
      "not a steam account!",
      "https://steamcommunity.com/profiles/123",
    ]);
    expect(body.failed[0].error).toMatch(/no Steam profile/);
    expect(body.failed[1].error).toMatch(/couldn't reach Steam/);
    // Each distinct name is asked once.
    const calls = (steamStub as unknown as { mock: { calls: unknown[][] } }).mock.calls.map((c) => String(c[0]));
    expect(calls.filter((u) => u.includes("/id/frostn1/")).length).toBe(1);
    expect(calls[0]).toBe("https://steamcommunity.com/id/frostn1/?xml=1");

    const grants = await env.DB.prepare("SELECT steam_id FROM entitlements WHERE asset_id = ? ORDER BY steam_id")
      .bind(assetId)
      .all<{ steam_id: string }>();
    expect(grants.results.map((r) => r.steam_id)).toEqual([BUYER, OTHER]);
  });

  it("parses without asking Steam where it can", () => {
    expect(parseSteamInput(` ${BUYER} `)).toEqual({ steamId: BUYER });
    expect(parseSteamInput(`http://www.steamcommunity.com/profiles/${BUYER}`)).toEqual({ steamId: BUYER });
    expect(parseSteamInput("https://steamcommunity.com/id/Some-Name_1/")).toEqual({ vanity: "Some-Name_1" });
    expect(parseSteamInput("https://example.com/id/x")).toHaveProperty("error");
  });

  it("404s an unknown asset and refuses bad bodies", async () => {
    const env = await deployment();
    const path = "/admin/assets/ast_nope/grants";
    expect((await call(env, req("GET", path))).status).toBe(404);
    expect((await call(env, req("POST", path, { body: { add: [BUYER] } }))).status).toBe(404);

    const { assetId } = await create(env);
    const mine = `/admin/assets/${assetId}/grants`;
    expect((await call(env, req("POST", mine, { body: { add: "x" } }))).status).toBe(400);
    expect((await call(env, req("POST", mine, { body: {} }))).status).toBe(400);
    const tooMany = Array.from({ length: 101 }, (_, i) => String(76561198000000100n + BigInt(i)));
    expect((await call(env, req("POST", mine, { body: { add: tooMany } }))).status).toBe(400);
  });

  it("fails an input over 256 characters without looking it up, and keeps the rest", async () => {
    const env = await deployment();
    const { assetId } = await create(env);
    const steam = vi.fn(steamStub);
    const long = `https://steamcommunity.com/id/frostn1/?${"x".repeat(300)}`;
    const res = await direct(
      env,
      req("POST", `/admin/assets/${assetId}/grants`, { body: { add: [BUYER, long] } }),
      steam as unknown as typeof fetch,
    );
    const body = (await res.json()) as { added: { steamId: string }[]; failed: { input: string; error: string }[] };
    expect(body.added.map((a) => a.steamId)).toEqual([BUYER]);
    expect(body.failed).toHaveLength(1);
    expect(body.failed[0].error).toMatch(/256/);
    expect(body.failed[0].input.length).toBe(257);
    expect(steam).not.toHaveBeenCalled();
  });
});

describe("POST /v1/keys/grant after an admin grant", () => {
  it("releases the same key to the granted Steam account, and stops on removal", async () => {
    const env = await deployment();
    const token = "buyer-token";
    await env.DB.prepare(
      "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)",
    )
      .bind("acc_buyer", "Buyer", BUYER, await hashToken(token), Date.now())
      .run();

    const created = await create(env);
    const ask = () =>
      call(env, req("POST", "/v1/keys/grant", { key: token, body: { assetId: created.assetId, sessionId: "s1" } }));

    expect((await ask()).status).toBe(403);

    await call(env, req("POST", `/admin/assets/${created.assetId}/grants`, { body: { add: [BUYER] } }));
    const granted = await ask();
    expect(granted.status).toBe(200);
    const body = (await granted.json()) as { contentKey: string; keyId: string };
    expect(body.contentKey).toBe(created.key);
    expect(body.keyId).toBe("k1");

    await call(env, req("POST", `/admin/assets/${created.assetId}/grants`, { body: { remove: [BUYER] } }));
    const after = await ask();
    expect(after.status).toBe(403);
    expect(await after.json()).toEqual({ error: "revoked" });
  });

  it("releases the key only for the registered file once its hash is set", async () => {
    const env = await deployment();
    const token = "buyer-token";
    await env.DB.prepare(
      "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)",
    )
      .bind("acc_buyer", "Buyer", BUYER, await hashToken(token), Date.now())
      .run();
    const created = await create(env);
    await call(env, req("POST", `/admin/assets/${created.assetId}/grants`, { body: { add: [BUYER] } }));

    const hash = "ab".repeat(32);
    const patch = (id: string, blobSha256: unknown) =>
      call(env, req("PATCH", `/admin/assets/${id}`, { body: { blobSha256 } }));
    expect((await patch(created.assetId, "nope")).status).toBe(400);
    expect((await patch("ast_missing", hash)).status).toBe(404);
    const set = await patch(created.assetId, hash.toUpperCase());
    expect(set.status).toBe(200);
    expect(await set.json()).toEqual({ assetId: created.assetId, blobSha256: hash, withdrawnAt: null, takenDownAt: null });

    const ask = (blobSha256?: string) =>
      call(env, req("POST", "/v1/keys/grant", { key: token, body: { assetId: created.assetId, sessionId: "s1", blobSha256 } }));
    expect((await ask(hash)).status).toBe(200);
    expect((await ask("cd".repeat(32))).status).not.toBe(200);
    expect((await ask()).status).not.toBe(200);
  });

  it("refuses malformed ids, and logs only a linked account asking about a real asset", async () => {
    const env = await deployment();
    const insert = "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)";
    await env.DB.prepare(insert).bind("acc_buyer", "Buyer", BUYER, await hashToken("buyer-token"), Date.now()).run();
    await env.DB.prepare(insert).bind("acc_nolink", "Nolink", null, await hashToken("nolink-token"), Date.now()).run();
    const created = await create(env);
    const ask = (token: string, body: Record<string, unknown>, path = "/v1/keys/grant") =>
      call(env, req("POST", path, { key: token, body }));

    const appSession = "0123456789abcdef0123456789abcdef";
    for (const body of [
      { assetId: "not an id!", sessionId: appSession },
      { assetId: "a".repeat(65), sessionId: appSession },
      { assetId: created.assetId, sessionId: "x".repeat(129) },
      { assetId: created.assetId, sessionId: "has a space" },
      { assetId: created.assetId, sessionId: "bell\u0007" },
    ]) {
      expect((await ask("buyer-token", body)).status, JSON.stringify(body).slice(0, 60)).toBe(400);
    }
    expect((await ask("buyer-token", { assetId: "bad id" }, "/v1/entitlements/check")).status).toBe(400);

    // What the app sends goes through to the decision; an old tool's shapes too.
    expect(await (await ask("buyer-token", { assetId: created.assetId, sessionId: appSession })).json()).toEqual({ error: "not entitled" });
    expect((await ask("buyer-token", { assetId: created.assetId, sessionId: "s1" })).status).toBe(403);
    expect((await ask("buyer-token", { assetId: created.assetId })).status).toBe(403);

    // Refused, and not written down: an asset that doesn't exist, and an account with no Steam.
    expect(await (await ask("buyer-token", { assetId: "trk_nope", sessionId: appSession })).json()).toEqual({ error: "no such asset" });
    expect(await (await ask("nolink-token", { assetId: created.assetId, sessionId: appSession })).json()).toEqual({
      error: "no Steam account linked",
    });

    const rows = await env.DB.prepare("SELECT steam_id, asset_id, session_id FROM entitlement_grants ORDER BY id").all();
    expect(rows.results).toEqual([
      { steam_id: BUYER, asset_id: created.assetId, session_id: appSession },
      { steam_id: BUYER, asset_id: created.assetId, session_id: "s1" },
      { steam_id: BUYER, asset_id: created.assetId, session_id: "none" },
    ]);
  });
});

describe("key grant rate limit", () => {
  it("limits /v1/keys/grant per account", async () => {
    const keys: string[] = [];
    const seen = new Map<string, number>();
    const KEY_GRANT_LIMITER = {
      limit: async ({ key }: { key: string }) => {
        keys.push(key);
        seen.set(key, (seen.get(key) ?? 0) + 1);
        return { success: seen.get(key)! <= 2 };
      },
    };
    const env = { ...(await deployment()), KEY_GRANT_LIMITER } as unknown as Env;
    const insert = "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)";
    await env.DB.prepare(insert).bind("acc_buyer", "Buyer", BUYER, await hashToken("buyer-token"), Date.now()).run();
    await env.DB.prepare(insert).bind("acc_other", "Other", OTHER, await hashToken("other-token"), Date.now()).run();
    const { assetId } = await create(env);
    const ask = (token: string) => call(env, req("POST", "/v1/keys/grant", { key: token, body: { assetId, sessionId: "s1" } }));

    expect((await ask("buyer-token")).status).toBe(403);
    expect((await ask("buyer-token")).status).toBe(403);
    const slow = await ask("buyer-token");
    expect(slow.status).toBe(429);
    expect(slow.headers.get("Retry-After")).toBe("60");
    expect(((await slow.json()) as { error: string }).error).toMatch(/too many key requests/);
    expect((await ask("other-token")).status).toBe(403);
    expect(keys).toEqual(["acc_buyer", "acc_buyer", "acc_buyer", "acc_other"]);
  });
});

describe("takedown", () => {
  it("is set and cleared only with a key, and a creator's restore doesn't lift it", async () => {
    const CREATOR = "76561198174305985";
    const env = await deployment({ MXB_WEB_SESSION_KEY: "session-secret" });
    await addAccount(env.DB, "acc_creator", "Creator", CREATOR);
    await env.DB.prepare("UPDATE accounts SET creator_at = 1 WHERE id = 'acc_creator'").run();
    await env.DB.prepare(
      "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)",
    )
      .bind("acc_buyer", "Buyer", BUYER, await hashToken("buyer-token"), Date.now())
      .run();
    const cookie = `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId: CREATOR, name: "C", exp: Date.now() + 60_000 }, "session-secret")}`;
    const asCreator = (method: string, path: string, body?: unknown) =>
      call(env, req(method, path, { key: null, origin: SITE, body, headers: { Cookie: cookie } }));

    const made = await asCreator("POST", "/admin/assets", { title: "Pine Hill" });
    const { assetId } = (await made.json()) as { assetId: string };
    await asCreator("POST", `/admin/assets/${assetId}/grants`, { add: [BUYER] });
    const ask = () => call(env, req("POST", "/v1/keys/grant", { key: "buyer-token", body: { assetId, sessionId: "s1" } }));
    const status = async () =>
      ((await (await call(env, req("POST", "/v1/assets/status", { key: "buyer-token", body: { assetIds: [assetId] } }))).json()) as {
        assets: { available: boolean }[];
      }).assets[0].available;
    expect((await ask()).status).toBe(200);

    const down = await call(env, req("PATCH", `/admin/assets/${assetId}`, { body: { takenDown: true } }));
    expect(down.status).toBe(200);
    expect(((await down.json()) as { takenDownAt: number | null }).takenDownAt).toBeTypeOf("number");
    expect(await (await ask()).json()).toEqual({ error: "taken down" });
    expect(await status()).toBe(false);

    // The creator can't clear it, directly or by toggling their own withdrawal.
    expect((await asCreator("PATCH", `/admin/assets/${assetId}`, { takenDown: false })).status).toBe(403);
    expect((await asCreator("PATCH", `/admin/assets/${assetId}`, { withdrawn: true })).status).toBe(200);
    expect((await asCreator("PATCH", `/admin/assets/${assetId}`, { withdrawn: false })).status).toBe(200);
    expect(await (await ask()).json()).toEqual({ error: "taken down" });

    // The creator sees it, on the list and the usage log.
    const listed = (await (await asCreator("GET", "/admin/assets")).json()) as { assets: { takenDownAt: number | null }[] };
    expect(listed.assets[0].takenDownAt).toBeTypeOf("number");
    const usage = (await (await asCreator("GET", `/admin/assets/${assetId}/usage`)).json()) as {
      takenDownAt: number | null;
      events: { reason: string }[];
    };
    expect(usage.takenDownAt).toBeTypeOf("number");
    expect(usage.events[0].reason).toBe("taken down");

    expect((await call(env, req("PATCH", `/admin/assets/${assetId}`, { body: { takenDown: "yes" } }))).status).toBe(400);
    const up = await call(env, req("PATCH", `/admin/assets/${assetId}`, { body: { takenDown: false } }));
    expect(((await up.json()) as { takenDownAt: number | null }).takenDownAt).toBeNull();
    expect((await ask()).status).toBe(200);
    expect(await status()).toBe(true);
  });
});

describe("buyer names", () => {
  it("adds Steam names to the buyer list and the usage log when asked", async () => {
    const env = await deployment();
    const created = await create(env);
    await call(env, req("POST", `/admin/assets/${created.assetId}/grants`, { body: { add: [BUYER] } }));
    const plain = (await (await direct(env, req("GET", `/admin/assets/${created.assetId}/grants`), steamStub)).json()) as {
      grants: { name?: string }[];
    };
    expect(plain.grants[0].name).toBeUndefined();
    const named = (await (await direct(env, req("GET", `/admin/assets/${created.assetId}/grants?names=1`), steamStub)).json()) as {
      grants: { steamId: string; name?: string }[];
    };
    expect(named.grants[0]).toMatchObject({ steamId: BUYER, name: "Buyer One" });
    const usage = await direct(env, req("GET", `/admin/assets/${created.assetId}/usage?names=1`), steamStub);
    expect(usage.status).toBe(200);
  });

  it("looks up at most 50 names; the rest come back blank", async () => {
    const env = await deployment();
    const created = await create(env);
    const ids = Array.from({ length: 60 }, (_, i) => String(76561198000001000n + BigInt(i)));
    await env.DB.batch(
      ids.map((id, i) =>
        env.DB.prepare("INSERT INTO entitlements (steam_id, asset_id, source, granted_at) VALUES (?, ?, 'grant', ?)").bind(
          id,
          created.assetId,
          1000 - i,
        ),
      ),
    );
    const steam = vi.fn(steamStub);
    const res = await direct(env, req("GET", `/admin/assets/${created.assetId}/grants?names=1`), steam as unknown as typeof fetch);
    const { grants } = (await res.json()) as { grants: { steamId: string; name: string }[] };
    expect(grants).toHaveLength(60);
    expect(grants.filter((g) => g.name === "Buyer One")).toHaveLength(50);
    expect(grants.slice(50).every((g) => g.name === "")).toBe(true);
    expect(steam).toHaveBeenCalledTimes(50);
  });
});
