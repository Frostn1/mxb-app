import { describe, expect, it, vi } from "vitest";
import { adminAssets } from "../src/assets";
import { landingSite, safeNext, webRoutes } from "../src/web";
import {
  LEGACY_SESSION_COOKIE,
  LOGIN_COOKIE,
  openToken,
  readCookie,
  sealToken,
  SESSION_COOKIE,
  webSession,
  type WebSession,
} from "../src/websession";
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

async function deployment(overrides: Record<string, string> = {}): Promise<Env> {
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
    ...overrides,
  } as unknown as Env;
}

async function cookieFor(steamId: string, exp = Date.now() + 60_000, name = SESSION_COOKIE): Promise<string> {
  return `${name}=${await sealToken({ t: "session", steamId, name: "Frost", exp }, KEY)}`;
}

/** A request the way the site sends it: from its own origin, JSON when there's a body. */
function req(
  method: string,
  path: string,
  opts: { cookie?: string; body?: unknown; origin?: string | null; contentType?: string } = {},
): Request {
  const headers: Record<string, string> = {};
  if (opts.cookie) headers.Cookie = opts.cookie;
  const origin = opts.origin === undefined ? SITE : opts.origin;
  if (origin) headers.Origin = origin;
  const type = opts.contentType ?? (opts.body !== undefined ? "application/json" : undefined);
  if (type) headers["Content-Type"] = type;
  return new Request(`${API}${path}`, {
    method,
    headers,
    body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
  });
}

const assets = (env: Env, r: Request) => adminAssets(r, new URL(r.url), env);
const web = (env: Env, r: Request, f?: typeof fetch) => webRoutes(r, new URL(r.url), env, f);

/** The `name=value` part of the Set-Cookie for `name`, if the response sets one. */
function setCookie(res: Response, name: string): string | undefined {
  return res.headers.getSetCookie().find((c) => c.startsWith(`${name}=`));
}

/** Steam confirming the assertion, and a profile whose name is Frost. */
const steamYes = vi.fn(async (input: RequestInfo | URL) =>
  String(input).includes("?xml=1")
    ? new Response("<profile><steamID><![CDATA[Frost]]></steamID></profile>")
    : new Response("ns:http://specs.openid.net/auth/2.0\nis_valid:true\n"),
) as unknown as typeof fetch;

/** Start a sign-in: the return URL Steam will send the browser to, and the cookie it was given. */
async function startLogin(env: Env, next = "/dashboard", site?: string): Promise<{ returnTo: URL; cookie: string }> {
  const from = site ? `&site=${encodeURIComponent(site)}` : "";
  const login = await web(env, req("GET", `/v1/web/steam/login?next=${encodeURIComponent(next)}${from}`, { origin: null }));
  const returnTo = new URL(new URL(login.headers.get("Location")!).searchParams.get("openid.return_to")!);
  return { returnTo, cookie: setCookie(login, LOGIN_COOKIE)!.split(";")[0] };
}

/** Steam sending the browser back, carrying whatever cookie it holds. */
function comeBack(returnTo: URL, steamId: string, cookie?: string): Request {
  const q = new URLSearchParams({
    "openid.ns": "http://specs.openid.net/auth/2.0",
    "openid.mode": "id_res",
    "openid.op_endpoint": "https://steamcommunity.com/openid/login",
    "openid.return_to": returnTo.href,
    "openid.claimed_id": `https://steamcommunity.com/openid/id/${steamId}`,
    "openid.identity": `https://steamcommunity.com/openid/id/${steamId}`,
    "openid.response_nonce": "2026-09-14T00:00:00Zabc",
    "openid.signed": "signed,op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle",
  });
  return req("GET", `/v1/web/steam/return?${returnTo.searchParams}&${q}`, { cookie, origin: null });
}

/** A whole sign-in in one browser, started at `next` on `site`. */
async function steamComesBack(env: Env, steamId: string, next = "/dashboard", site?: string): Promise<Request> {
  const { returnTo, cookie } = await startLogin(env, next, site);
  return comeBack(returnTo, steamId, cookie);
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

  it("reads a cookie out of the header, preferring the first name asked for", () => {
    const r = new Request(API, { headers: { Cookie: "a=1; mxb_session=old; b=2; __Host-mxb_session=new" } });
    expect(readCookie(r, "mxb_session")).toBe("old");
    expect(readCookie(r, "missing")).toBeNull();
    expect(readCookie(r, "__Host-mxb_session", "mxb_session")).toBe("new");
    expect(readCookie(r, "missing", "b")).toBe("2");
  });

  it("reads the __Host- session first, and the old name only when it's absent", async () => {
    const env = await deployment();
    const both = new Request(API, {
      headers: { Cookie: `${await cookieFor(OTHER, undefined, LEGACY_SESSION_COOKIE)}; ${await cookieFor(CREATOR)}` },
    });
    expect((await webSession(both, env))?.steamId).toBe(CREATOR);
    const old = new Request(API, { headers: { Cookie: await cookieFor(OTHER, undefined, LEGACY_SESSION_COOKIE) } });
    expect((await webSession(old, env))?.steamId).toBe(OTHER);
  });

  it("only ever lands on a path on the site", () => {
    expect(safeNext("/dashboard")).toBe("/dashboard");
    expect(safeNext("/lock?tab=x")).toBe("/lock?tab=x");
    for (const bad of ["//evil.com", "https://evil.com", "/\\evil.com", "", null]) expect(safeNext(bad)).toBe("/dashboard");
  });
});

describe("Steam sign-in", () => {
  it("sends the browser to Steam with a signed state, and ties it to this browser", async () => {
    const res = await web(await deployment(), req("GET", "/v1/web/steam/login?next=/lock", { origin: null }));
    expect(res.status).toBe(302);
    const to = new URL(res.headers.get("Location")!);
    expect(to.origin).toBe("https://steamcommunity.com");
    const returnTo = new URL(to.searchParams.get("openid.return_to")!);
    expect(returnTo.origin + returnTo.pathname).toBe(`${API}/v1/web/steam/return`);
    const state = await openToken<{ t: "state"; n: string; next: string; exp: number }>(returnTo.searchParams.get("state"), KEY, "state");
    expect(state).toMatchObject({ next: "/lock" });

    const login = setCookie(res, LOGIN_COOKIE)!;
    expect(login.split(";")[0]).toBe(`${LOGIN_COOKIE}=${state!.n}`);
    for (const flag of ["HttpOnly", "Secure", "SameSite=Lax", "Path=/", "Max-Age=600"]) expect(login).toContain(flag);
    expect(login).not.toContain("Domain");
  });

  it("confirms with Steam, sets the cookie and lands back on the site", async () => {
    const env = await deployment();
    const res = await web(env, await steamComesBack(env, CREATOR), steamYes);
    expect(res.status).toBe(302);
    expect(res.headers.get("Location")).toBe(`${SITE}/dashboard`);
    const cookie = setCookie(res, SESSION_COOKIE)!;
    for (const flag of ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"]) expect(cookie).toContain(flag);
    expect(cookie).not.toContain("Domain");
    // The sign-in is used up, and a session under the old name is replaced.
    expect(setCookie(res, LOGIN_COOKIE)).toContain("Max-Age=0");
    expect(setCookie(res, LEGACY_SESSION_COOKIE)).toContain("Max-Age=0");

    const me = await web(env, req("GET", "/v1/web/me", { cookie: cookie.split(";")[0] }));
    expect(me.status).toBe(200);
    expect(me.headers.get("Access-Control-Allow-Origin")).toBe(SITE);
    expect(me.headers.get("Access-Control-Allow-Credentials")).toBe("true");
    expect(await me.json()).toEqual({ steamId: CREATOR, name: "Frost", creator: true, linked: true });
  });

  it("refuses a return that didn't start in this browser, before asking Steam", async () => {
    const env = await deployment();
    const steam = vi.fn(steamYes);
    const { returnTo } = await startLogin(env);
    const { cookie: someoneElses } = await startLogin(env);

    for (const cookie of [undefined, someoneElses, `${LOGIN_COOKIE}=`]) {
      const res = await web(env, comeBack(returnTo, CREATOR, cookie), steam as unknown as typeof fetch);
      expect(res.status).toBe(403);
      expect(setCookie(res, SESSION_COOKIE)).toBeUndefined();
      // Left alone, so a sign-in this browser really has in flight still completes.
      expect(setCookie(res, LOGIN_COOKIE)).toBeUndefined();
    }
    expect(steam).not.toHaveBeenCalled();
  });

  it("lands back on www when the sign-in started there, never on another site", async () => {
    const env = await deployment();
    const www = await web(env, await steamComesBack(env, CREATOR, "/lock", "https://www.mxbsecure.com"), steamYes);
    expect(www.headers.get("Location")).toBe("https://www.mxbsecure.com/lock");
    const evil = await web(env, await steamComesBack(env, CREATOR, "/lock", "https://evil.com"), steamYes);
    expect(evil.headers.get("Location")).toBe(`${SITE}/lock`);
    expect(landingSite(null, env)).toBe(SITE);
  });

  it("lands on a local build only when dev origins are switched on", async () => {
    expect(landingSite("http://localhost:5173", await deployment())).toBe(SITE);
    const dev = await deployment({ MXB_ALLOW_DEV_ORIGINS: "1" });
    expect(landingSite("http://localhost:5173", dev)).toBe("http://localhost:5173");
    const res = await web(dev, await steamComesBack(dev, CREATOR, "/lock", "http://127.0.0.1:5173"), steamYes);
    expect(res.headers.get("Location")).toBe("http://127.0.0.1:5173/lock");
  });

  it("refuses a broken state, and an assertion Steam won't confirm", async () => {
    const env = await deployment();
    expect((await web(env, req("GET", "/v1/web/steam/return?state=junk", { origin: null }))).status).toBe(400);
    const no = vi.fn(async () => new Response("is_valid:false\n")) as unknown as typeof fetch;
    const res = await web(env, await steamComesBack(env, CREATOR), no);
    expect(res.status).toBe(403);
    expect(setCookie(res, SESSION_COOKIE)).toBeUndefined();
    expect(setCookie(res, LOGIN_COOKIE)).toContain("Max-Age=0");
  });

  it("answers /me 401 when signed out, and logout clears both cookie names", async () => {
    const env = await deployment();
    expect((await web(env, req("GET", "/v1/web/me"))).status).toBe(401);
    const out = await web(env, req("POST", "/v1/web/logout", { body: {} }));
    expect(out.status).toBe(204);
    expect(out.headers.get("Access-Control-Allow-Origin")).toBe(SITE);
    expect(setCookie(out, SESSION_COOKIE)).toContain("Max-Age=0");
    expect(setCookie(out, LEGACY_SESSION_COOKIE)).toContain("Max-Age=0");
  });

  it("refuses a logout that didn't come from the site, as JSON", async () => {
    const env = await deployment();
    const cookie = await cookieFor(CREATOR);
    for (const r of [
      req("POST", "/v1/web/logout", { cookie, body: {}, origin: null }),
      req("POST", "/v1/web/logout", { cookie, body: {}, origin: "https://evil.mxbsecure.com" }),
      req("POST", "/v1/web/logout", { cookie, contentType: "text/plain" }),
    ]) {
      const res = await web(env, r);
      expect(res.status).toBe(403);
      expect(res.headers.getSetCookie()).toEqual([]);
    }
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

  it("can only write from the site's own origin, as JSON", async () => {
    const env = await deployment();
    const frost = await cookieFor(CREATOR);
    const made = await assets(env, req("POST", "/admin/assets", { cookie: frost, body: { title: "Pine Hill" } }));
    const { assetId } = (await made.json()) as { assetId: string };
    const grants = `/admin/assets/${assetId}/grants`;
    const body = { add: [OTHER] };

    for (const r of [
      req("POST", grants, { cookie: frost, body, origin: null }),
      req("POST", grants, { cookie: frost, body, origin: "https://evil.mxbsecure.com" }),
      req("POST", grants, { cookie: frost, body, contentType: "text/plain" }),
      req("POST", grants, { cookie: frost, body, contentType: "application/x-www-form-urlencoded" }),
      req("PATCH", `/admin/assets/${assetId}`, { cookie: frost, body: { withdrawn: true }, origin: null }),
      req("POST", "/admin/assets", { cookie: frost, body: { title: "Sneaky" }, contentType: "text/plain" }),
    ]) {
      expect((await assets(env, r)).status).toBe(403);
    }
    const count = await env.DB.prepare("SELECT (SELECT COUNT(*) FROM entitlements) AS e, (SELECT COUNT(*) FROM assets) AS a").first<{ e: number; a: number }>();
    expect(count).toEqual({ e: 0, a: 1 });
    const withdrawn = await env.DB.prepare("SELECT withdrawn_at FROM assets WHERE id = ?").bind(assetId).first<{ withdrawn_at: number | null }>();
    expect(withdrawn?.withdrawn_at).toBeNull();

    // Reads need neither, and the site's own JSON write goes through.
    expect((await assets(env, req("GET", grants, { cookie: frost, origin: null }))).status).toBe(200);
    expect((await assets(env, req("POST", grants, { cookie: frost, body, contentType: "application/json; charset=utf-8" }))).status).toBe(200);
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
        " VALUES (?, ?, 's1', 'allow', 'entitled', 10), (?, ?, 's2', 'deny', 'revoked', 20)," +
        " ('unlinked', ?, 's3', 'deny', 'no Steam account linked', 30)",
    )
      .bind(OTHER, assetId, OTHER, assetId, assetId)
      .run();

    const usage = (await (await assets(env, req("GET", `/admin/assets/${assetId}/usage`, { cookie: frost }))).json()) as {
      buyers: unknown[];
      events: unknown[];
    };
    // The old "unlinked" row isn't a buyer, and doesn't count as the last request.
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
