import { describe, expect, it, vi } from "vitest";
import { adminAssets } from "../src/assets";
import { landingSite, safeNext, webRoutes } from "../src/web";
import { openToken, readCookie, sealToken, SESSION_COOKIE, type WebSession } from "../src/websession";
import { addAccount, d1 } from "./d1sqlite";

const KEY = "session-secret";
const CREATOR = "76561198174305985";
const OTHER = "76561198000000043";
const SITE = "https://mxbsecure.com";
const API = "https://api.mxbsecure.com";

function masterKey(): string {
  let s = "";
  for (const b of crypto.getRandomValues(new Uint8Array(32))) s += String.fromCharCode(b);
  return btoa(s);
}

async function deployment(): Promise<Env> {
  const DB = d1();
  await addAccount(DB, "acc_owner", "Owner");
  await addAccount(DB, "acc_frost", "Frost", CREATOR);
  await addAccount(DB, "acc_other", "Other", OTHER);
  await DB.prepare("UPDATE accounts SET creator_at = 1 WHERE id IN ('acc_frost', 'acc_other')").run();
  return {
    DB,
    MXB_WEB_SESSION_KEY: KEY,
    MXB_ASSET_MASTER_KEY: masterKey(),
    MXB_OWNER_ACCOUNT_ID: "acc_owner",
    MXB_SITE_ORIGIN: SITE,
  } as unknown as Env;
}

async function cookieFor(steamId: string, exp = Date.now() + 60_000): Promise<string> {
  return `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId, name: "Frost", exp }, KEY)}`;
}

function req(method: string, path: string, opts: { cookie?: string; body?: unknown; origin?: string } = {}): Request {
  const headers: Record<string, string> = {};
  if (opts.cookie) headers.Cookie = opts.cookie;
  if (opts.origin) headers.Origin = opts.origin;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";
  return new Request(`${API}${path}`, {
    method,
    headers,
    body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
  });
}

const assets = (env: Env, r: Request) => adminAssets(r, new URL(r.url), env);
const web = (env: Env, r: Request, f?: typeof fetch) => webRoutes(r, new URL(r.url), env, f);

/** Steam confirming the assertion, and a profile whose name is Frost. */
const steamYes = vi.fn(async (input: RequestInfo | URL) =>
  String(input).includes("?xml=1")
    ? new Response("<profile><steamID><![CDATA[Frost]]></steamID></profile>")
    : new Response("ns:http://specs.openid.net/auth/2.0\nis_valid:true\n"),
) as unknown as typeof fetch;

/** The return URL Steam would send the browser to, for a login started at `next` on `site`. */
async function steamComesBack(env: Env, steamId: string, next = "/dashboard", site?: string): Promise<string> {
  const from = site ? `&site=${encodeURIComponent(site)}` : "";
  const login = await web(env, req("GET", `/v1/web/steam/login?next=${encodeURIComponent(next)}${from}`));
  const returnTo = new URL(new URL(login.headers.get("Location")!).searchParams.get("openid.return_to")!);
  const q = new URLSearchParams({
    "openid.ns": "http://specs.openid.net/auth/2.0",
    "openid.mode": "id_res",
    "openid.return_to": returnTo.href,
    "openid.claimed_id": `https://steamcommunity.com/openid/id/${steamId}`,
    "openid.identity": `https://steamcommunity.com/openid/id/${steamId}`,
  });
  return `/v1/web/steam/return?${returnTo.searchParams}&${q}`;
}

describe("signed tokens", () => {
  it("round-trip, and refuse tampering, the wrong type, another key and expiry", async () => {
    const t = await sealToken({ t: "session", steamId: CREATOR, name: "x", exp: Date.now() + 1000 }, KEY);
    expect((await openToken<WebSession>(t, KEY, "session"))?.steamId).toBe(CREATOR);
    expect(await openToken(t, KEY, "state")).toBeNull();
    expect(await openToken(t, "another-key", "session")).toBeNull();
    const [body, sig] = t.split(".");
    expect(await openToken(`${body}x.${sig}`, KEY, "session")).toBeNull();
    const old = await sealToken({ t: "session", steamId: CREATOR, name: "x", exp: Date.now() - 1 }, KEY);
    expect(await openToken(old, KEY, "session")).toBeNull();
  });

  it("reads a cookie out of the header", () => {
    const r = new Request(API, { headers: { Cookie: "a=1; mxb_session=tok; b=2" } });
    expect(readCookie(r, "mxb_session")).toBe("tok");
    expect(readCookie(r, "missing")).toBeNull();
  });

  it("only ever lands on a path on the site", () => {
    expect(safeNext("/dashboard")).toBe("/dashboard");
    expect(safeNext("/lock?tab=x")).toBe("/lock?tab=x");
    for (const bad of ["//evil.com", "https://evil.com", "/\\evil.com", "", null]) expect(safeNext(bad)).toBe("/dashboard");
  });
});

describe("Steam sign-in", () => {
  it("sends the browser to Steam with a signed state", async () => {
    const res = await web(await deployment(), req("GET", "/v1/web/steam/login?next=/lock"));
    expect(res.status).toBe(302);
    const to = new URL(res.headers.get("Location")!);
    expect(to.origin).toBe("https://steamcommunity.com");
    const returnTo = new URL(to.searchParams.get("openid.return_to")!);
    expect(returnTo.origin + returnTo.pathname).toBe(`${API}/v1/web/steam/return`);
    expect(await openToken(returnTo.searchParams.get("state"), KEY, "state")).toMatchObject({ next: "/lock" });
  });

  it("confirms with Steam, sets the cookie and lands back on the site", async () => {
    const env = await deployment();
    const res = await web(env, req("GET", await steamComesBack(env, CREATOR)), steamYes);
    expect(res.status).toBe(302);
    expect(res.headers.get("Location")).toBe(`${SITE}/dashboard`);
    const cookie = res.headers.get("Set-Cookie")!;
    for (const flag of ["HttpOnly", "Secure", "SameSite=Lax"]) expect(cookie).toContain(flag);

    const me = await web(env, req("GET", "/v1/web/me", { cookie: cookie.split(";")[0], origin: SITE }));
    expect(me.status).toBe(200);
    expect(me.headers.get("Access-Control-Allow-Origin")).toBe(SITE);
    expect(me.headers.get("Access-Control-Allow-Credentials")).toBe("true");
    expect(await me.json()).toEqual({ steamId: CREATOR, name: "Frost", creator: true, linked: true });
  });

  it("lands back on www when the sign-in started there, never on another site", async () => {
    const env = await deployment();
    const www = await web(env, req("GET", await steamComesBack(env, CREATOR, "/lock", "https://www.mxbsecure.com")), steamYes);
    expect(www.headers.get("Location")).toBe("https://www.mxbsecure.com/lock");
    const evil = await web(env, req("GET", await steamComesBack(env, CREATOR, "/lock", "https://evil.com")), steamYes);
    expect(evil.headers.get("Location")).toBe(`${SITE}/lock`);
    expect(landingSite(null, env)).toBe(SITE);
  });

  it("refuses a broken state, and an assertion Steam won't confirm", async () => {
    const env = await deployment();
    expect((await web(env, req("GET", "/v1/web/steam/return?state=junk"))).status).toBe(400);
    const no = vi.fn(async () => new Response("is_valid:false\n")) as unknown as typeof fetch;
    const res = await web(env, req("GET", await steamComesBack(env, CREATOR)), no);
    expect(res.status).toBe(403);
    expect(res.headers.get("Set-Cookie")).toBeNull();
  });

  it("answers /me 401 when signed out, and logout clears the cookie", async () => {
    const env = await deployment();
    expect((await web(env, req("GET", "/v1/web/me"))).status).toBe(401);
    const out = await web(env, req("POST", "/v1/web/logout", { origin: SITE }));
    expect(out.status).toBe(204);
    expect(out.headers.get("Set-Cookie")).toContain("Max-Age=0");
  });
});

describe("creators on /admin/assets", () => {
  it("see and change only their own assets", async () => {
    const env = await deployment();
    const frost = await cookieFor(CREATOR);
    const other = await cookieFor(OTHER);
    const made = await assets(env, req("POST", "/admin/assets", { cookie: frost, body: { title: "Pine Hill" } }));
    expect(made.status).toBe(201);
    const { assetId } = (await made.json()) as { assetId: string };
    const row = await env.DB.prepare("SELECT creator_id FROM assets WHERE id = ?").bind(assetId).first<{ creator_id: string }>();
    expect(row?.creator_id).toBe("acc_frost");

    const mine = (await (await assets(env, req("GET", "/admin/assets", { cookie: frost }))).json()) as { assets: { assetId: string }[] };
    expect(mine.assets.map((a) => a.assetId)).toEqual([assetId]);
    const theirs = (await (await assets(env, req("GET", "/admin/assets", { cookie: other }))).json()) as { assets: unknown[] };
    expect(theirs.assets).toEqual([]);
    for (const path of [`/admin/assets/${assetId}/grants`, `/admin/assets/${assetId}/usage`]) {
      expect((await assets(env, req("GET", path, { cookie: other }))).status).toBe(404);
    }
    expect((await assets(env, req("PATCH", `/admin/assets/${assetId}`, { cookie: other, body: { withdrawn: true } }))).status).toBe(404);
  });

  it("refuses a signed-in account that isn't a creator, and an expired session", async () => {
    const env = await deployment();
    await env.DB.prepare("UPDATE accounts SET creator_at = NULL WHERE id = 'acc_other'").run();
    expect((await assets(env, req("GET", "/admin/assets", { cookie: await cookieFor(OTHER) }))).status).toBe(403);
    const expired = await cookieFor(CREATOR, Date.now() - 1);
    expect((await assets(env, req("GET", "/admin/assets", { cookie: expired }))).status).toBe(401);
  });

  it("shows whose app asked for the key, and withdraws and restores", async () => {
    const env = await deployment();
    const frost = await cookieFor(CREATOR);
    const made = await assets(env, req("POST", "/admin/assets", { cookie: frost, body: { title: "Pine Hill" } }));
    const { assetId } = (await made.json()) as { assetId: string };
    await env.DB.prepare(
      "INSERT INTO entitlement_grants (steam_id, asset_id, session_id, decision, reason, issued_at)" +
        " VALUES (?, ?, 's1', 'allow', 'entitled', 10), (?, ?, 's2', 'deny', 'revoked', 20)",
    )
      .bind(OTHER, assetId, OTHER, assetId)
      .run();

    const usage = (await (await assets(env, req("GET", `/admin/assets/${assetId}/usage`, { cookie: frost }))).json()) as {
      buyers: unknown[];
      events: unknown[];
    };
    expect(usage.buyers).toEqual([{ steamId: OTHER, unlocks: 1, refused: 1, lastAt: 20 }]);
    expect(usage.events).toHaveLength(2);

    const listed = (await (await assets(env, req("GET", "/admin/assets", { cookie: frost }))).json()) as {
      assets: { lastRequestAt: number | null; hashed: boolean }[];
    };
    expect(listed.assets[0]).toMatchObject({ lastRequestAt: 20, hashed: false });

    const off = await assets(env, req("PATCH", `/admin/assets/${assetId}`, { cookie: frost, body: { withdrawn: true } }));
    expect(((await off.json()) as { withdrawnAt: number | null }).withdrawnAt).not.toBeNull();
    const on = await assets(env, req("PATCH", `/admin/assets/${assetId}`, { cookie: frost, body: { withdrawn: false } }));
    expect(((await on.json()) as { withdrawnAt: number | null }).withdrawnAt).toBeNull();
    expect((await assets(env, req("PATCH", `/admin/assets/${assetId}`, { cookie: frost, body: {} }))).status).toBe(400);
  });
});
