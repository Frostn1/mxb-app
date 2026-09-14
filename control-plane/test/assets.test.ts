import { describe, expect, it, vi } from "vitest";
import { unwrapContentKey } from "../src/assetkey";
import { adminAssets, parseSteamInput } from "../src/assets";
import { hashToken } from "../src/auth";
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

  it("503s when the admin key, master key or owner is unset", async () => {
    for (const unset of ["ADMIN_KEY", "MXB_ASSET_MASTER_KEY", "MXB_OWNER_ACCOUNT_ID"]) {
      const env = await deployment({ [unset]: undefined });
      const res = await call(env, req("GET", "/admin/assets"));
      expect(res.status, unset).toBe(503);
    }
  });
});

describe("CORS", () => {
  it("answers an allowed preflight 204 before auth", async () => {
    const env = await deployment();
    for (const origin of [SITE, "https://www.mxbsecure.com", "http://localhost:5173"]) {
      const res = await call(
        env,
        req("OPTIONS", "/admin/assets/ast_x/grants", {
          key: null,
          origin,
          headers: { "Access-Control-Request-Method": "POST" },
        }),
      );
      expect(res.status).toBe(204);
      expect(res.headers.get("Access-Control-Allow-Origin")).toBe(origin);
      expect(res.headers.get("Access-Control-Allow-Methods")).toBe("GET, POST, OPTIONS");
      expect(res.headers.get("Access-Control-Allow-Headers")).toBe("Authorization, Content-Type");
      expect(res.headers.get("Vary")).toContain("Origin");
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
});
