import { describe, expect, it, vi } from "vitest";
import { adminAssets, CREATOR_SIGNUP_NEEDED } from "../src/assets";
import { landingSite, safeNext, webRoutes } from "../src/web";
import { adminSteamIds } from "../src/webadmin";
import { SIGNUP_CLOSED } from "../src/creators";
import { addBan, BANNED } from "../src/bans";
import { CONVERTER_NOT_GRANTED } from "../src/converter";
import { LOCK_GUIDS_PER_HOUR, LOCK_REFUSED, MAX_PERMIT_GUIDS, pruneLockAttempts } from "../src/lockpermit";
import { guidFromSteamId } from "../src/steam";
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
    // Enough R2 for the locker route: `get` returns something with a body, or null.
    LOCKWEB: {
      objects: new Map<string, string>(),
      async get(name: string) {
        const body = (this as { objects: Map<string, string> }).objects.get(name);
        return body === undefined ? null : { body };
      },
    },
    // The converter's bucket, the same fake as the locker's.
    FBX2EDF: {
      objects: new Map<string, string>(),
      async get(name: string) {
        const body = (this as { objects: Map<string, string> }).objects.get(name);
        return body === undefined ? null : { body };
      },
    },
    // The paints bucket, as far as the paint views ask: a digest nobody uploaded is absent.
    PAINTS: { async head() { return null; }, async get() { return null; } },
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
    expect(await me.json()).toEqual({
      steamId: CREATOR,
      name: "Frost",
      creator: true,
      linked: true,
      locks: { usedToday: 0, perDay: 10, remaining: 10 },
      admin: false,
      // Not an admin and not on the converter list, which `deployment()` leaves empty.
      converter: false,
      // Closed unless the deployment opens it, which `deployment()` does not.
      creatorSignup: "closed",
    });
  });

  it("refuses a return that didn't start in this browser, before asking Steam", async () => {
    const env = await deployment();
    const steam = vi.fn(steamYes);
    const { returnTo } = await startLogin(env);
    const { cookie: someoneElses } = await startLogin(env);

    for (const cookie of [undefined, someoneElses, `${LOGIN_COOKIE}=`]) {
      const res = await web(env, comeBack(returnTo, CREATOR, cookie), steam as unknown as typeof fetch);
      expect(res.status).toBe(303);
      expect(res.headers.get("Location")).toBe(`${SITE}/steam?r=other-browser`);
      expect(setCookie(res, SESSION_COOKIE)).toBeUndefined();
      // Left alone, so a sign-in this browser really has in flight still completes.
      expect(setCookie(res, LOGIN_COOKIE)).toBeUndefined();
    }
    expect(steam).not.toHaveBeenCalled();
  });

  it("limits sign-in per client address, login and return together", async () => {
    const seen = new Map<string, number>();
    const SIGNIN_LIMITER = {
      limit: async ({ key }: { key: string }) => {
        seen.set(key, (seen.get(key) ?? 0) + 1);
        return { success: seen.get(key)! <= 2 };
      },
    };
    const env = { ...(await deployment()), SIGNIN_LIMITER } as unknown as Env;
    const from = (ip: string, path: string) => new Request(`${API}${path}`, { headers: { "CF-Connecting-IP": ip } });

    expect((await web(env, from("1.2.3.4", "/v1/web/steam/login"))).status).toBe(302);
    expect((await web(env, from("1.2.3.4", "/v1/web/steam/return?state=junk"))).headers.get("Location")).toBe(`${SITE}/steam?r=expired`);
    const slow = await web(env, from("1.2.3.4", "/v1/web/steam/login"));
    expect(slow.status).toBe(303);
    expect(slow.headers.get("Location")).toBe(`${SITE}/steam?r=busy`);
    expect((await web(env, from("5.6.7.8", "/v1/web/steam/login"))).status).toBe(302);
    // /me and logout aren't counted.
    expect((await web(env, from("1.2.3.4", "/v1/web/me"))).status).toBe(401);
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
    expect((await web(env, req("GET", "/v1/web/steam/return?state=junk", { origin: null }))).headers.get("Location")).toBe(`${SITE}/steam?r=expired`);
    const no = vi.fn(async () => new Response("is_valid:false\n")) as unknown as typeof fetch;
    const res = await web(env, await steamComesBack(env, CREATOR), no);
    expect(res.headers.get("Location")).toBe(`${SITE}/steam?r=unconfirmed`);
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

  it("keeps a signed-in stranger out until they sign up, and lets existing creators in", async () => {
    const env = await deployment();
    const cookie = await cookieFor("76561198000000077");
    expect((await assets(env, req("GET", "/admin/assets", { cookie }))).status).toBe(403);
    expect((await assets(env, req("POST", "/admin/assets", { cookie, body: { title: "X" } }))).status).toBe(403);
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({ creator: false });
    expect((await env.DB.prepare("SELECT COUNT(*) AS n FROM accounts WHERE steam_id = '76561198000000077'").first<{ n: number }>())?.n).toBe(0);
    expect((await assets(env, req("GET", "/admin/assets", { cookie: await cookieFor(CREATOR) }))).status).toBe(200);
  });

  it("still knows a creator whose steam_id has been lost, and puts it back", async () => {
    const env = await deployment();
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES ('acc_frost', ?, 1)")
      .bind(CREATOR)
      .run();
    // The failure this guards: the column cleared under a creator who is still linked as far as
    // Valve is concerned. Without the link log they are a stranger to their own dashboard.
    await env.DB.prepare("UPDATE accounts SET steam_id = NULL WHERE id = 'acc_frost'").run();
    const cookie = await cookieFor(CREATOR);

    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({
      creator: true,
      linked: true,
    });
    expect((await assets(env, req("GET", "/admin/assets", { cookie }))).status).toBe(200);
    const back = await env.DB.prepare("SELECT steam_id FROM accounts WHERE id = 'acc_frost'").first<{
      steam_id: string | null;
    }>();
    expect(back?.steam_id).toBe(CREATOR);
  });

  it("does not invent a creator out of a Steam account that never linked", async () => {
    const env = await deployment();
    const cookie = await cookieFor("76561198000000077");
    expect((await assets(env, req("GET", "/admin/assets", { cookie }))).status).toBe(403);
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({ creator: false });
  });

  it("lets a creator lock 10 new files a day, and the owner any number", async () => {
    const make = (env: Env, cookie: string) => assets(env, req("POST", "/admin/assets", { cookie, body: { title: "T" } }));
    const cookie = await cookieFor(CREATOR);
    const env = await deployment();
    for (let i = 0; i < 10; i++) expect((await make(env, cookie)).status).toBe(201);
    const over = await make(env, cookie);
    expect(over.status).toBe(429);
    expect(((await over.json()) as { error: string }).error).toBe("You can lock 10 new files a day. Try again tomorrow.");
    const owner = await deployment({ MXB_OWNER_ACCOUNT_ID: "acc_frost" });
    for (let i = 0; i < 11; i++) expect((await make(owner, cookie)).status).toBe(201);
  });

  it("never lets a stale copy of who-you-are be reused", async () => {
    const env = await deployment();
    const res = await web(env, req("GET", "/v1/web/me", { cookie: await cookieFor(CREATOR) }));
    expect(res.status).toBe(200);
    // Creator status changes the instant an account is granted it; a cached "creator: false"
    // outlives the grant and is indistinguishable from a real refusal.
    expect(res.headers.get("cache-control")).toBe("no-store");
  });

  it("never makes a creator out of someone who merely signed in with Steam", async () => {
    const env = await deployment();
    const NEWCOMER = "76561198000000077";
    const cookie = await cookieFor(NEWCOMER);
    const profile = () =>
      env.DB.prepare("SELECT id FROM accounts WHERE steam_id = ?").bind(NEWCOMER).first<{ id: string }>();

    // Signing in is proof of who they are, never of what they may sell. Signing up is one
    // click, and it is still a click somebody has to make.
    expect((await assets(env, req("GET", "/admin/assets", { cookie }))).status).toBe(403);
    const made = await assets(env, req("POST", "/admin/assets", { cookie, body: { title: "First" } }));
    expect(made.status).toBe(403);
    expect(((await made.json()) as { error: string }).error).toBe("sign up as a creator on mxbsecure.com first");
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({ creator: false });
    // No profile is conjured on the way past, so nothing accretes creator status later.
    expect(await profile()).toBeNull();
  });

  it("signs a rider up as a creator on the spot, and remembers it was their own doing", async () => {
    // With the front door open, which is a deployment choice and no longer the default.
    const env = await deployment({ MXB_CREATOR_SIGNUP: "open" });
    const NEWCOMER = "76561198000000077";
    const cookie = await cookieFor(NEWCOMER);

    const up = await web(env, req("POST", "/v1/web/creator", { cookie, body: {} }));
    expect(up.status).toBe(201);
    expect(await up.json()).toMatchObject({ creator: true, already: false });
    expect(up.headers.get("cache-control")).toBe("no-store");

    // A creator from that moment: the locking routes open with nothing else to do.
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({ creator: true });
    expect((await assets(env, req("POST", "/admin/assets", { cookie, body: { title: "First" } }))).status).toBe(201);

    const row = await env.DB.prepare("SELECT kind, creator_at, creator_source FROM accounts WHERE steam_id = ?")
      .bind(NEWCOMER)
      .first<{ kind: string; creator_at: number; creator_source: string }>();
    expect(row).toMatchObject({ kind: "web", creator_source: "self" });

    // Asking twice changes nothing, and never a second account.
    const again = await web(env, req("POST", "/v1/web/creator", { cookie, body: {} }));
    expect(again.status).toBe(200);
    expect(await again.json()).toMatchObject({ already: true });
    expect((await env.DB.prepare("SELECT COUNT(*) AS n FROM accounts WHERE steam_id = ?").bind(NEWCOMER).first<{ n: number }>())?.n).toBe(1);
  });

  it("signs nobody up who isn't signed in, or whose request didn't come from the site", async () => {
    const env = await deployment();
    expect((await web(env, req("POST", "/v1/web/creator", { body: {} }))).status).toBe(401);
    const cookie = await cookieFor("76561198000000077");
    // A form post from a sibling subdomain: the cookie rides along, the Origin cannot.
    const forged = await web(env, req("POST", "/v1/web/creator", { cookie, origin: null, contentType: "text/plain" }));
    expect(forged.status).toBe(403);
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({ creator: false });
  });

  it("takes no new creators when the door is shut, and says so in words", async () => {
    // The default: no `MXB_CREATOR_SIGNUP` at all. A deployment that was never told either way
    // does not hold the door open.
    const env = await deployment();
    const NEWCOMER = "76561198000000077";
    const cookie = await cookieFor(NEWCOMER);

    const shut = await web(env, req("POST", "/v1/web/creator", { cookie, body: {} }));
    expect(shut.status).toBe(403);
    expect(await shut.json()).toMatchObject({ error: SIGNUP_CLOSED });
    // Refused, and nothing left behind: no standing, and no web-only account made for one.
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({
      creator: false,
      creatorSignup: "closed",
    });
    expect(
      (await env.DB.prepare("SELECT COUNT(*) AS n FROM accounts WHERE steam_id = ?").bind(NEWCOMER).first<{ n: number }>())?.n,
    ).toBe(0);
    // And the locking routes are shut with it, which is the point of the door.
    expect((await assets(env, req("POST", "/admin/assets", { cookie, body: { title: "First" } }))).status).toBe(403);
  });

  it("takes nothing from the creators who are already through it", async () => {
    const env = await deployment();
    const cookie = await cookieFor(CREATOR);
    // A reload, or a browser retrying the post: they are a creator, and the shut door says so
    // rather than telling them they may not be what they already are.
    const again = await web(env, req("POST", "/v1/web/creator", { cookie, body: {} }));
    expect(again.status).toBe(200);
    expect(await again.json()).toMatchObject({ creator: true, already: true });
    expect(again.headers.get("cache-control")).toBe("no-store");
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()).toMatchObject({ creator: true });
    expect((await assets(env, req("POST", "/admin/assets", { cookie, body: { title: "Still" } }))).status).toBe(201);
  });

  it("opens only for the exact word, so a typo leaves it shut", async () => {
    const cookie = await cookieFor("76561198000000077");
    for (const value of ["", "1", "yes", "OPEN ", "Open", "closed"]) {
      const env = await deployment({ MXB_CREATOR_SIGNUP: value });
      const said = await web(env, req("POST", "/v1/web/creator", { cookie, body: {} }));
      // "OPEN " and "Open" are the word, trimmed and case-folded; the rest are not.
      const opens = value.trim().toLowerCase() === "open";
      expect([value, said.status]).toEqual([value, opens ? 201 : 403]);
    }
  });

  it("refuses a banned rider before it refuses anybody for the door being shut", async () => {
    // Both refusals are 403; which one they are told matters. "Banned" is the honest answer the
    // website owes them, and "we aren't taking creators" would send them to ask us by hand.
    const env = await deployment({ MXB_CREATOR_SIGNUP: "open" });
    const BANNED_STEAM = "76561198000000077";
    await addBan(env, { guid: guidFromSteamId(BANNED_STEAM), reason: "unlocked and shared protected content" }, CREATOR);
    const cookie = await cookieFor(BANNED_STEAM);
    const said = await web(env, req("POST", "/v1/web/creator", { cookie, body: {} }));
    expect(said.status).toBe(403);
    expect(await said.json()).toMatchObject({ error: BANNED });
  });

  it("says what is left of today's ceiling, and says the owner has none", async () => {
    const env = await deployment();
    const cookie = await cookieFor(CREATOR);
    const mine = async () => ((await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()) as { locks?: unknown }).locks;
    expect(await mine()).toEqual({ usedToday: 0, perDay: 10, remaining: 10 });
    for (let i = 0; i < 3; i++) await assets(env, req("POST", "/admin/assets", { cookie, body: { title: "T" } }));
    expect(await mine()).toEqual({ usedToday: 3, perDay: 10, remaining: 7 });

    // Somebody who hasn't signed up has no ceiling to report, because they cannot lock at all.
    const stranger = await cookieFor("76561198000000077");
    expect(await (await web(env, req("GET", "/v1/web/me", { cookie: stranger }))).json()).not.toHaveProperty("locks");

    const owner = await deployment({ MXB_OWNER_ACCOUNT_ID: "acc_frost" });
    const theirs = (await (await web(owner, req("GET", "/v1/web/me", { cookie }))).json()) as { locks: unknown };
    expect(theirs.locks).toEqual({ usedToday: 0, perDay: null, remaining: null });
  });

  it("hands the locker to a creator, and to nobody else", async () => {
    const env = await deployment();
    // Not signed in.
    expect((await web(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js"))).status).toBe(401);
    // Signed in, nothing uploaded: a configuration problem, not a missing page.
    const frost = await cookieFor(CREATOR);
    expect((await web(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js", { cookie: frost }))).status).toBe(503);
    // A name that was never servable, whoever asks.
    expect((await web(env, req("GET", "/v1/web/lockweb/../secrets", { cookie: frost }))).status).toBe(404);
    expect((await web(env, req("GET", "/v1/web/lockweb/anything.txt", { cookie: frost }))).status).toBe(404);

    // Uploaded: served as a module, and never at a shared cache.
    (env as unknown as { LOCKWEB: { objects: Map<string, string> } }).LOCKWEB.objects.set("mxb_lockweb.js", "export default 1");
    const got = await web(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js", { cookie: frost }));
    expect(got.status).toBe(200);
    expect(got.headers.get("content-type")).toBe("text/javascript; charset=utf-8");
    expect(got.headers.get("cache-control")).toBe("private, max-age=3600");
    expect(await got.text()).toBe("export default 1");

    // Signed in but not a creator: both locks are creators-only, and the refusal says which
    // step is missing rather than reading as a locker that isn't there.
    const rider = await cookieFor("76561198000000077");
    const refused = await web(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js", { cookie: rider }));
    expect(refused.status).toBe(403);
    expect(await refused.json()).toEqual({ error: CREATOR_SIGNUP_NEEDED });
    expect((await assets(env, req("GET", "/admin/assets", { cookie: rider }))).status).toBe(403);
    // An expired session is not a session.
    const stale = await cookieFor(CREATOR, Date.now() - 1);
    expect((await web(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js", { cookie: stale }))).status).toBe(401);
  });

  it("hands the converter to granted accounts and admins, and to nobody else", async () => {
    const GRANTED = "76561198000000077";
    const env = await deployment({ MXB_CONVERTER_STEAM_IDS: `${GRANTED}, not-a-steam-id`, MXB_ADMIN_STEAM_IDS: CREATOR });
    const file = "/v1/web/fbx2edf/fbx2edf.js";
    // Not signed in.
    expect((await web(env, req("GET", file))).status).toBe(401);
    // Signed in without the permission — a creator is not thereby a converter.
    const other = await cookieFor(OTHER);
    const refused = await web(env, req("GET", file, { cookie: other }));
    expect(refused.status).toBe(403);
    expect(await refused.json()).toEqual({ error: CONVERTER_NOT_GRANTED });
    // Granted, nothing uploaded yet: a configuration problem, not a missing page.
    const granted = await cookieFor(GRANTED);
    expect((await web(env, req("GET", file, { cookie: granted }))).status).toBe(503);
    // Names outside the closed list, whoever asks.
    expect((await web(env, req("GET", "/v1/web/fbx2edf/../secrets", { cookie: granted }))).status).toBe(404);
    expect((await web(env, req("GET", "/v1/web/fbx2edf/fbx2edf.txt", { cookie: granted }))).status).toBe(404);

    (env as unknown as { FBX2EDF: { objects: Map<string, string> } }).FBX2EDF.objects.set("fbx2edf.js", "export default 1");
    const got = await web(env, req("GET", file, { cookie: granted }));
    expect(got.status).toBe(200);
    expect(got.headers.get("content-type")).toBe("text/javascript; charset=utf-8");
    expect(got.headers.get("cache-control")).toBe("no-store");
    expect(await got.text()).toBe("export default 1");
    // Names that are only inherited properties of the allowlist are not files.
    for (const bogus of ["constructor", "__proto__", "toString"]) {
      expect((await web(env, req("GET", `/v1/web/fbx2edf/${bogus}`, { cookie: granted }))).status).toBe(404);
    }
    // An admin always may.
    expect((await web(env, req("GET", file, { cookie: await cookieFor(CREATOR) }))).status).toBe(200);

    // What /me tells the page, so it can draw the tool or the invite-only note.
    const me = async (cookie: string) => ((await (await web(env, req("GET", "/v1/web/me", { cookie }))).json()) as { converter: boolean }).converter;
    expect(await me(granted)).toBe(true);
    expect(await me(other)).toBe(false);

    // A ban takes it away, list or no list.
    await addBan(env, { guid: guidFromSteamId(GRANTED), reason: "unlocked and shared protected content" }, CREATOR);
    const banned = await web(env, req("GET", file, { cookie: granted }));
    expect(banned.status).toBe(403);
    expect(await banned.json()).toEqual({ error: BANNED });
    expect(await me(granted)).toBe(false);
  });

  it("gives the converter to nobody when no bucket is bound", async () => {
    const env = await deployment({ MXB_ADMIN_STEAM_IDS: CREATOR });
    delete (env as unknown as { FBX2EDF?: unknown }).FBX2EDF;
    expect((await web(env, req("GET", "/v1/web/fbx2edf/fbx2edf_bg.wasm", { cookie: await cookieFor(CREATOR) }))).status).toBe(503);
  });

  it("refuses an expired session", async () => {
    const env = await deployment();
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

describe("creator API keys", () => {
  const BUYER = "76561198000000099";
  const withKey = (key: string, method: string, path: string, body?: unknown) =>
    new Request(`${API}${path}`, {
      method,
      headers: { Authorization: `Bearer ${key}`, ...(body === undefined ? {} : { "Content-Type": "application/json" }) },
      body: body === undefined ? undefined : JSON.stringify(body),
    });

  it("lets a shop's server list its creator's assets and change buyers, and nothing else", async () => {
    const env = await deployment();
    const frost = await cookieFor(CREATOR);
    const { assetId } = (await (await assets(env, req("POST", "/admin/assets", { cookie: frost, body: { title: "Pine Hill" } }))).json()) as { assetId: string };
    const { assetId: theirs } = (await (
      await assets(env, req("POST", "/admin/assets", { cookie: await cookieFor(OTHER), body: { title: "Not yours" } }))
    ).json()) as { assetId: string };

    const made = await assets(env, req("POST", "/admin/api-keys", { cookie: frost, body: { label: "mxbikes-shop" } }));
    expect(made.status).toBe(201);
    const { id, key } = (await made.json()) as { id: string; key: string };
    expect(key).toMatch(/^mxbs_/);
    const listed = (await (await assets(env, req("GET", "/admin/api-keys", { cookie: frost }))).json()) as { keys: unknown[] };
    expect(listed.keys).toEqual([expect.objectContaining({ id, label: "mxbikes-shop" })]);
    expect(JSON.stringify(listed)).not.toContain(key);

    const list = await assets(env, withKey(key, "GET", "/admin/assets"));
    expect(list.status).toBe(200);
    expect(((await list.json()) as { assets: { assetId: string }[] }).assets.map((a) => a.assetId)).toEqual([assetId]);
    expect((await assets(env, withKey(key, "POST", `/admin/assets/${assetId}/grants`, { add: [BUYER] }))).status).toBe(200);
    expect((await assets(env, withKey(key, "GET", `/admin/assets/${assetId}/grants`))).status).toBe(200);

    for (const r of [
      withKey(key, "POST", "/admin/assets", { title: "X" }),
      withKey(key, "PATCH", `/admin/assets/${assetId}`, { withdrawn: true }),
      withKey(key, "DELETE", `/admin/assets/${assetId}?keys=keep`),
      withKey(key, "GET", `/admin/assets/${assetId}/usage`),
      withKey(key, "POST", "/admin/api-keys", { label: "more" }),
      withKey(key, "GET", "/admin/api-keys"),
    ]) {
      expect((await assets(env, r)).status).toBe(403);
    }
    expect((await assets(env, withKey(key, "POST", `/admin/assets/${theirs}/grants`, { add: [BUYER] }))).status).toBe(404);

    expect((await assets(env, req("POST", `/admin/api-keys/${id}/revoke`, { cookie: frost, body: {} }))).status).toBe(200);
    expect((await assets(env, withKey(key, "GET", "/admin/assets"))).status).toBe(401);
    expect(((await (await assets(env, req("GET", "/admin/api-keys", { cookie: frost }))).json()) as { keys: unknown[] }).keys).toEqual([]);
  });

  it("refuses a made-up key, more than 10 keys, and keys whose owner stopped being a creator", async () => {
    const env = await deployment();
    expect((await assets(env, withKey(`mxbs_${"x".repeat(43)}`, "GET", "/admin/assets"))).status).toBe(401);
    const frost = await cookieFor(CREATOR);
    const make = () => assets(env, req("POST", "/admin/api-keys", { cookie: frost, body: { label: "k" } }));
    const { key } = (await (await make()).json()) as { key: string };
    for (let i = 1; i < 10; i++) expect((await make()).status).toBe(201);
    expect((await make()).status).toBe(409);
    expect((await assets(env, withKey(key, "GET", "/admin/assets"))).status).toBe(200);
    await env.DB.prepare("UPDATE accounts SET creator_at = NULL WHERE id = 'acc_frost'").run();
    expect((await assets(env, withKey(key, "GET", "/admin/assets"))).status).toBe(401);
  });
});

describe("the dashboards on the site", () => {
  const ADMINS = { MXB_ADMIN_STEAM_IDS: CREATOR };

  it("is nobody's until the deployment names them", async () => {
    const env = await deployment();
    expect(adminSteamIds(env)).toEqual([]);
    // Being a creator is not being an admin: one sells through the site, the other reads
    // everybody's numbers.
    expect((await web(env, req("GET", "/v1/web/admin/usage", { cookie: await cookieFor(CREATOR) }))).status).toBe(403);
  });

  it("drops anything in the list that isn't a SteamID64", async () => {
    const env = await deployment({ MXB_ADMIN_STEAM_IDS: `nonsense, ${CREATOR} ${OTHER}, 12` });
    expect(adminSteamIds(env)).toEqual([CREATOR, OTHER]);
  });

  it("asks for a sign-in, then for the right one", async () => {
    const env = await deployment(ADMINS);
    expect((await web(env, req("GET", "/v1/web/admin/usage"))).status).toBe(401);
    expect((await web(env, req("GET", "/v1/web/admin/usage", { cookie: await cookieFor(OTHER) }))).status).toBe(403);
    // Expired reads as signed out, not as refused: the fix is to sign in again.
    const stale = await cookieFor(CREATOR, Date.now() - 1000);
    expect((await web(env, req("GET", "/v1/web/admin/usage", { cookie: stale }))).status).toBe(401);
  });

  it("hands an admin the same numbers the rendered page draws", async () => {
    const env = await deployment(ADMINS);
    const res = await web(env, req("GET", "/v1/web/admin/usage?days=7", { cookie: await cookieFor(CREATOR) }));
    expect(res.status).toBe(200);
    expect(res.headers.get("Cache-Control")).toBe("no-store");
    expect(res.headers.get("Access-Control-Allow-Origin")).toBe(SITE);
    const stats = (await res.json()) as { days: number; daily: unknown[]; active: { day: number } };
    expect(stats.days).toBe(7);
    expect(stats.active.day).toBe(0);
    expect(Array.isArray(stats.daily)).toBe(true);
  });

  it("serves the diagnostics views the rendered pages serve", async () => {
    const env = await deployment(ADMINS);
    const frost = await cookieFor(CREATOR);
    const get = async (path: string) => {
      const res = await web(env, req("GET", path, { cookie: frost }));
      return { status: res.status, body: (await res.json()) as Record<string, unknown> };
    };

    const overview = await get("/v1/web/admin/diagnostics?days=7");
    expect(overview.status).toBe(200);
    // The rules ride along with the overview rather than costing a second round trip.
    expect(overview.body).toMatchObject({ days: 7, live: [], rules: [], reporting: 0 });
    expect(overview.body.totals).toMatchObject({ accounts: 3 });

    expect(await get("/v1/web/admin/diagnostics/riders?q=frost")).toMatchObject({
      status: 200,
      body: { rows: [{ riderName: "Frost" }], page: 1 },
    });
    expect((await get("/v1/web/admin/diagnostics/files")).body).toMatchObject({ rows: [], total: 0 });

    // A name nobody has is a 404, not an empty detail page.
    expect((await get("/v1/web/admin/diagnostics/rider?who=nobody")).status).toBe(404);
    expect((await get("/v1/web/admin/diagnostics/rider")).status).toBe(404);
    expect((await get("/v1/web/admin/diagnostics/file?name=nothing.dll")).status).toBe(404);
  });

  it("writes a rule only from the site, and only a usable one", async () => {
    const env = await deployment(ADMINS);
    const frost = await cookieFor(CREATOR);
    const post = (body: unknown, opts: Record<string, unknown> = {}) =>
      web(env, req("POST", "/v1/web/admin/diagnostics/rules", { cookie: frost, body, ...opts }));

    // A form post from somewhere else is refused before the rule is read.
    expect((await post({ kind: "deny", pattern: "x.dll" }, { origin: "https://evil.example" })).status).toBe(403);
    expect((await post({ kind: "sideways", pattern: "x.dll" })).status).toBe(400);
    // A name *and* a hash reads two different ways; `addRule` refuses it and so does this.
    expect((await post({ kind: "deny", pattern: "x.dll", sha256: "a".repeat(64) })).status).toBe(400);

    expect((await post({ kind: "deny", pattern: "cheat.dll", label: "Known cheat" })).status).toBe(200);
    const after = await web(env, req("GET", "/v1/web/admin/diagnostics", { cookie: frost }));
    const { rules } = (await after.json()) as { rules: { id: number; pattern: string }[] };
    expect(rules).toMatchObject([{ pattern: "cheat.dll" }]);

    expect((await post({ action: "delete", id: rules[0].id })).status).toBe(200);
    expect((await post({ action: "delete", id: 0 })).status).toBe(400);
  });

  it("serves the paint views, and refuses a digest that isn't one", async () => {
    const env = await deployment(ADMINS);
    const frost = await cookieFor(CREATOR);
    const get = async (path: string) => {
      const res = await web(env, req("GET", path, { cookie: frost }));
      return { status: res.status, body: (await res.json()) as Record<string, unknown> };
    };

    const riders = await get("/v1/web/admin/paints/riders");
    expect(riders.status).toBe(200);
    expect(riders.body.totals).toMatchObject({ riders: 0, paints: 0 });
    expect(riders.body.found).toMatchObject({ rows: [], page: 1 });
    // The column asked for is looked up, never trusted: a hand-edited sort is the default.
    expect((await get("/v1/web/admin/paints/riders?sort=nonsense")).body.order).toMatchObject({ sort: "published" });
    expect((await get("/v1/web/admin/paints/files")).status).toBe(200);

    expect((await get("/v1/web/admin/paints/rider?id=nobody")).status).toBe(404);
    expect((await get("/v1/web/admin/paints/paint?sha=not-a-digest")).status).toBe(404);
    expect((await get(`/v1/web/admin/paints/paint?sha=${"a".repeat(64)}`)).status).toBe(404);
  });

  it("mints, revokes and grants, and refuses what the data layer refuses", async () => {
    const env = await deployment(ADMINS);
    const frost = await cookieFor(CREATOR);
    await env.DB.prepare("INSERT INTO plugins (id, name, created_at) VALUES ('voice', 'Voice', 1)").run();
    const post = (body: unknown, opts: Record<string, unknown> = {}) =>
      web(env, req("POST", "/v1/web/admin/plugins", { cookie: frost, body, ...opts }));
    const read = async (path: string) =>
      (await (await web(env, req("GET", path, { cookie: frost }))).json()) as Record<string, never>;

    expect((await post({ action: "mint", plugin: "voice", months: 3, count: 2 }, { origin: "https://evil.example" })).status).toBe(403);
    expect((await post({ action: "sideways" })).status).toBe(400);
    // The ceilings are `mintKeys`'s, not this endpoint's: one place decides what is sane.
    expect((await post({ action: "mint", plugin: "voice", months: 99, count: 2 })).status).toBe(400);
    expect((await post({ action: "mint", plugin: "nope", months: 3, count: 2 })).status).toBe(400);

    const minted = await post({ action: "mint", plugin: "voice", months: 3, count: 2, note: "testers" });
    expect(minted.status).toBe(200);
    const { at } = (await minted.json()) as { at: number };

    const keys = await read(`/v1/web/admin/plugins/keys?minted=${at}`);
    expect((keys.batch as unknown as string[]).length).toBe(2);
    expect((keys.found as unknown as { rows: { code: string }[] }).rows).toHaveLength(2);
    // The migrations ship a plugin of their own, so this is "contains", not "equals".
    expect(keys.plugins as unknown as { id: string; keys: number }[]).toContainEqual(
      expect.objectContaining({ id: "voice", keys: 2 }),
    );

    const code = (keys.batch as unknown as string[])[0];
    expect((await post({ action: "key-revoke", code })).status).toBe(200);
    const revoked = await read("/v1/web/admin/plugins/keys?state=revoked");
    expect((revoked.found as unknown as { total: number }).total).toBe(1);
    expect((await post({ action: "key-restore", code })).status).toBe(200);

    expect((await post({ action: "grant", who: "nobody", plugin: "voice", months: 3 })).status).toBe(400);
    expect((await post({ action: "grant", who: "Frost", plugin: "voice", months: 3 })).status).toBe(200);
    expect((await read("/v1/web/admin/plugins/licenses")).found).toMatchObject({ total: 1 });
    expect((await post({ action: "license-revoke", account: "acc_frost", plugin: "voice" })).status).toBe(200);
    expect((await read("/v1/web/admin/plugins/licenses?state=live")).found).toMatchObject({ total: 0 });
  });

  it("adds and removes creators, making a web-only row for someone with no app account", async () => {
    const env = await deployment(ADMINS);
    const frost = await cookieFor(CREATOR);
    const LINCAO = "76561198209325252";
    const RIDER = "76561198000000077";
    await addAccount(env.DB, "acc_rider", "Rider", RIDER);
    const steam = (async (input: RequestInfo | URL) => {
      const u = String(input);
      if (u.includes("/id/gstavlincon/")) return new Response(`<profile><steamID64>${LINCAO}</steamID64></profile>`);
      if (u.includes(`/profiles/${LINCAO}/`)) return new Response("<profile><steamID><![CDATA[Lincão]]></steamID></profile>");
      return new Response("", { status: 404 });
    }) as typeof fetch;
    const post = (body: unknown, opts: Record<string, unknown> = {}) =>
      web(env, req("POST", "/v1/web/admin/creators", { cookie: frost, body, ...opts }), steam);
    const list = async () =>
      ((await (await web(env, req("GET", "/v1/web/admin/creators", { cookie: frost }), steam)).json()) as {
        creators: { accountId: string; steamId: string; steamName: string; linked: boolean }[];
      }).creators;

    expect((await post({ action: "add", who: LINCAO }, { origin: "https://evil.example" })).status).toBe(403);
    expect((await post({ action: "sideways" })).status).toBe(400);
    expect((await post({ action: "add", who: "not a profile!" })).status).toBe(400);

    // Nobody in the app has this Steam account: a web-only row stands in until they link it.
    const added = await post({ action: "add", who: "https://steamcommunity.com/id/gstavlincon/" });
    expect(added.status).toBe(200);
    expect(await added.json()).toMatchObject({ steamId: LINCAO, already: false });
    expect(await list()).toContainEqual(expect.objectContaining({ steamId: LINCAO, steamName: "Lincão", linked: false }));
    expect(await (await post({ action: "add", who: LINCAO })).json()).toMatchObject({ already: true });

    // An app account with that Steam ID is promoted in place, not duplicated.
    expect(await (await post({ action: "add", who: RIDER })).json()).toMatchObject({ accountId: "acc_rider", already: false });
    expect(await list()).toContainEqual(expect.objectContaining({ accountId: "acc_rider", linked: true }));

    expect((await post({ action: "remove", account: "acc_rider" })).status).toBe(200);
    expect((await post({ action: "remove", account: "acc_rider" })).status).toBe(404);
    expect((await list()).map((c) => c.accountId)).not.toContain("acc_rider");
  });

  it("clamps the window and refuses a path it doesn't serve", async () => {
    const env = await deployment(ADMINS);
    const frost = await cookieFor(CREATOR);
    const res = await web(env, req("GET", "/v1/web/admin/usage?days=9000", { cookie: frost }));
    expect(((await res.json()) as { days: number }).days).toBe(365);
    expect((await web(env, req("GET", "/v1/web/admin/nothing", { cookie: frost }))).status).toBe(404);
  });

  it("tells the site whether to offer them at all", async () => {
    const admin = await deployment(ADMINS);
    const me = async (env: Env, steamId: string) =>
      (await (await web(env, req("GET", "/v1/web/me", { cookie: await cookieFor(steamId) }))).json()) as {
        admin: boolean;
        creator: boolean;
      };
    expect(await me(admin, CREATOR)).toMatchObject({ admin: true, creator: true });
    expect(await me(admin, OTHER)).toMatchObject({ admin: false, creator: true });
    expect(await me(await deployment(), CREATOR)).toMatchObject({ admin: false });
  });
});

describe("the GUID lock's permit", () => {
  // Synthetic GUIDs only (`scripts/guid-allowlist.txt`): this repository is public.
  const CLEAN = "FF0110000111111111";
  const BANNED_TARGET = "FF0110000122222222";
  const refusedBody = { error: LOCK_REFUSED };
  const permit = async (
    env: Env,
    guids: unknown,
    opts: { cookie?: string; origin?: string | null; contentType?: string } = {},
  ) => web(env, req("POST", "/v1/web/lock/permit", { cookie: await cookieFor(CREATOR), body: { guids }, ...opts }));
  /** `n` distinct GUIDs of the right shape, all under the allowlisted synthetic prefix. */
  const many = (n: number) => Array.from({ length: n }, (_, i) => `FF0110000${String(i).padStart(9, "0")}`);

  /** Every "lock permit refused" line logged while `run` ran, parsed. */
  async function logged(run: () => Promise<unknown>): Promise<Record<string, unknown>[]> {
    const spy = vi.spyOn(console, "log").mockImplementation(() => undefined);
    try {
      await run();
      return spy.mock.calls
        .map((c) => String(c[0]))
        .filter((line) => line.includes("lock permit refused"))
        .map((line) => JSON.parse(line) as Record<string, unknown>);
    } finally {
      spy.mockRestore();
    }
  }

  it("lets a creator lock to a clean GUID, normalised", async () => {
    const env = await deployment();
    const ok = await permit(env, [` ${CLEAN.toLowerCase()} `]);
    expect(ok.status).toBe(200);
    expect(await ok.json()).toEqual({ ok: true });
    expect(ok.headers.get("cache-control")).toBe("no-store");
    expect(ok.headers.get("access-control-allow-origin")).toBe(SITE);
  });

  it("refuses a banned GUID with the lock page's own words, and logs why", async () => {
    const env = await deployment();
    await addBan(env, { guid: BANNED_TARGET, reason: "test" }, "admin");
    let res!: Response;
    const lines = await logged(async () => {
      res = await permit(env, [CLEAN, BANNED_TARGET]);
    });
    expect(res.status).toBe(403);
    const text = await res.text();
    expect(JSON.parse(text)).toEqual(refusedBody);
    expect(text.toLowerCase()).not.toContain("ban");
    expect(lines).toEqual([
      {
        msg: "lock permit refused",
        reason: "banned_target",
        account: "acc_frost",
        steamId: CREATOR,
        guids: [CLEAN, BANNED_TARGET],
        banned: [BANNED_TARGET],
      },
    ]);
    // Lifted is not banned.
    await env.DB.prepare("UPDATE guid_bans SET lifted_at = 1 WHERE guid = ?").bind(BANNED_TARGET).run();
    expect((await permit(env, [BANNED_TARGET])).status).toBe(200);
  });

  it("trips after the hour's allowance with the identical refusal", async () => {
    const env = await deployment();
    await addBan(env, { guid: BANNED_TARGET, reason: "test" }, "admin");
    const banned = await (await permit(env, [BANNED_TARGET])).text();
    // The banned probe above spent one of the hour, like any other ask.
    let left = LOCK_GUIDS_PER_HOUR - 1;
    while (left > 0) {
      const n = Math.min(left, MAX_PERMIT_GUIDS);
      expect((await permit(env, many(n))).status).toBe(200);
      left -= n;
    }
    let res!: Response;
    const lines = await logged(async () => {
      res = await permit(env, [CLEAN]);
    });
    expect(res.status).toBe(403);
    // Byte for byte what a banned GUID is told, so the two cannot be told apart.
    expect(await res.text()).toBe(banned);
    expect(lines).toEqual([expect.objectContaining({ reason: "rate_limited", account: "acc_frost", guids: [CLEAN] })]);
    // A refused ask spends nothing: the count stands at the ceiling, not over it.
    const used = await env.DB.prepare("SELECT SUM(guids) AS n FROM lock_attempts WHERE account_id = 'acc_frost'").first<{
      n: number;
    }>();
    expect(used?.n).toBe(LOCK_GUIDS_PER_HOUR);

    // It was only ever this creator's hour; and an hour on, it is open again.
    expect((await permit(env, [CLEAN], { cookie: await cookieFor(OTHER) })).status).toBe(200);
    await env.DB.prepare("UPDATE lock_attempts SET attempted_at = attempted_at - ?").bind(60 * 60 * 1000 + 1).run();
    expect((await permit(env, [CLEAN])).status).toBe(200);
  });

  it("has no ceiling for the owner account", async () => {
    const env = await deployment({ MXB_OWNER_ACCOUNT_ID: "acc_frost" });
    for (let i = 0; i < 3; i++) expect((await permit(env, many(MAX_PERMIT_GUIDS))).status).toBe(200);
  });

  it("refuses without a session, off the site, to a non-creator, a bad list and a banned caller", async () => {
    const env = await deployment();
    const anon = await web(env, req("POST", "/v1/web/lock/permit", { body: { guids: [CLEAN] } }));
    expect(anon.status).toBe(401);
    expect(await anon.json()).toEqual(refusedBody);

    // Another origin, or a body a page could send without a preflight.
    for (const offSite of [{ origin: "https://evil.example" }, { contentType: "text/plain" }]) {
      const res = await permit(env, [CLEAN], offSite);
      expect(res.status).toBe(403);
      expect(await res.json()).toEqual(refusedBody);
    }
    // The preflight is answered for the site and nobody else.
    const pre = await web(env, req("OPTIONS", "/v1/web/lock/permit"));
    expect(pre.status).toBe(204);
    expect(pre.headers.get("access-control-allow-origin")).toBe(SITE);
    expect((await web(env, req("OPTIONS", "/v1/web/lock/permit", { origin: "https://evil.example" }))).status).toBe(403);

    const rider = await permit(env, [CLEAN], { cookie: await cookieFor("76561198000000077") });
    expect(rider.status).toBe(403);
    expect(await rider.json()).toEqual(refusedBody);

    for (const bad of [[], CLEAN, ["not-a-guid"], ["FF01100001111111"], many(MAX_PERMIT_GUIDS + 1)]) {
      const res = await permit(env, bad);
      expect(res.status).toBe(400);
      expect(await res.json()).toEqual(refusedBody);
    }

    // A banned creator is refused the same way, whatever they ask to lock to.
    await addBan(env, { guid: guidFromSteamId(CREATOR), reason: "test" }, "admin");
    let res!: Response;
    const lines = await logged(async () => {
      res = await permit(env, [CLEAN]);
    });
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual(refusedBody);
    expect(lines).toEqual([expect.objectContaining({ reason: "caller_banned", steamId: CREATOR })]);
  });

  it("forgets attempts after a day", async () => {
    const env = await deployment();
    expect((await permit(env, [CLEAN])).status).toBe(200);
    await pruneLockAttempts(env, Date.now() + 24 * 60 * 60 * 1000 + 1);
    const left = await env.DB.prepare("SELECT COUNT(*) AS n FROM lock_attempts").first<{ n: number }>();
    expect(left?.n).toBe(0);
  });
});
