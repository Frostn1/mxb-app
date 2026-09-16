/**
 * Banning a rider from mxbsecure, and the identities a ban has to follow.
 *
 * Against the real router and a real SQLite (`d1sqlite.ts`), because the whole of this feature
 * *is* the resolution query: a ban that only matched the GUID column on one account would pass
 * any stub and mean nothing the first time somebody reinstalls.
 */

import { describe, expect, it, vi } from "vitest";
import { APP_BLOCK_MESSAGE, addBan, appGate, banFor, liftBan, listBans, normalizeGuid, rememberGuid } from "../src/bans";
import { guidFromSteamId } from "../src/steam";
import { hashToken } from "../src/auth";
import { mintKeys } from "../src/plugins";
import { sealToken, SESSION_COOKIE } from "../src/websession";
import { addAccount, d1, publishBundle } from "./d1sqlite";

// The entry module exports the voice Durable Object, whose base class only exists in workerd.
vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const ADMIN = "s3cret";
const SESSION = "session-secret";
const OWNER = "acc_owner";
const SITE = "https://mxbsecure.com";
/** The Steam account the dashboards are open to, and the one they are not. */
const BOSS = "76561198174305985";
const BUYER = "76561198000000042";
const CLEAN = "76561198000000077";
/**
 * 18 hex characters, the shape the locker normalises to. Deliberately not one of the six the
 * deployment ships banned — those are covered on their own, and a test that reused one would
 * pass whether or not anything here worked.
 */
const GUID = "AA0110000100000001";
const OTHER_GUID = "AA0110000100000002";

function masterKey(): string {
  let s = "";
  for (const b of crypto.getRandomValues(new Uint8Array(32))) s += String.fromCharCode(b);
  return btoa(s);
}

/** A real Ed25519 private key, base64url pkcs8 — what `PLUGIN_SIGNING_KEY` holds. */
async function pluginSigningKey(): Promise<string> {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])) as CryptoKeyPair;
  const pkcs8 = new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer);
  let binary = "";
  for (const b of pkcs8) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function deployment(overrides: Record<string, string> = {}): Promise<Env> {
  const DB = d1();
  await addAccount(DB, OWNER, "Owner");
  return {
    DB,
    ADMIN_KEY: ADMIN,
    MXB_ASSET_MASTER_KEY: masterKey(),
    MXB_OWNER_ACCOUNT_ID: OWNER,
    MXB_WEB_SESSION_KEY: SESSION,
    MXB_SITE_ORIGIN: SITE,
    MXB_ADMIN_STEAM_IDS: BOSS,
    PLUGIN_SIGNING_KEY: await pluginSigningKey(),
    // The plugin bundle lives in the same R2 the paints do, as far as `plugins.ts` asks.
    PAINTS: { async get(key: string) { return key ? { body: "BUNDLE" } : null; } },
    // Enough R2 for the locker route: one file, present.
    LOCKWEB: { async get(name: string) { return name ? { body: "wasm" } : null; } },
  } as unknown as Env;
}

function req(
  method: string,
  path: string,
  opts: { body?: unknown; key?: string | null; cookie?: string; origin?: string | null } = {},
): Request {
  const headers: Record<string, string> = {};
  if (opts.key) headers.Authorization = `Bearer ${opts.key}`;
  if (opts.cookie) headers.Cookie = opts.cookie;
  const origin = opts.origin === undefined ? SITE : opts.origin;
  if (origin) headers.Origin = origin;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";
  return new Request(`https://cp.test${path}`, {
    method,
    headers,
    body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
  });
}

const call = (env: Env, request: Request) => worker.fetch(request, env, {} as ExecutionContext);

const cookie = async (steamId: string) =>
  `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId, name: "Rider", exp: Date.now() + 60_000 }, SESSION)}`;

/** An app account with a bearer token, the way every client arrives. */
async function account(env: Env, id: string, token: string, steamId: string | null, guid: string | null) {
  await env.DB.prepare(
    "INSERT INTO accounts (id, rider_name, steam_id, guid, token_hash, created_at) VALUES (?, ?, ?, ?, ?, ?)",
  )
    .bind(id, id, steamId, guid, await hashToken(token), Date.now())
    .run();
}

/** A packed asset the buyer owns, so a grant has something real to refuse. */
async function ownedAsset(env: Env, steamId: string): Promise<string> {
  const res = await call(env, req("POST", "/admin/assets", { key: ADMIN, body: { title: "Pine Hill" }, origin: null }));
  expect(res.status).toBe(201);
  const { assetId } = (await res.json()) as { assetId: string };
  await env.DB.prepare(
    "INSERT INTO entitlements (steam_id, asset_id, source, granted_at) VALUES (?, ?, 'grant', ?)",
  )
    .bind(steamId, assetId, Date.now())
    .run();
  return assetId;
}

/** A creator's API key, in the shape `assets.ts` recognises. */
const CREATOR_KEY = "mxbs_0123456789abcdefghijklmn";

/** One plugin key for `replaycam`, and the code that redeems it. */
async function mintOne(env: Env): Promise<string> {
  const minted = await mintKeys(env, "replaycam", 1, 1, "ban test");
  expect(minted.ok).toBe(true);
  return minted.codes[0];
}

const ban = (env: Env, guid: string, reason = "unlocked and shared protected content") =>
  addBan(env, { guid, reason }, BOSS);

describe("the ban list the deployment ships with", () => {
  it("bans the six reported installs, and nothing else", async () => {
    const env = await deployment();
    const rows = await listBans(env);
    expect(rows.map((r) => r.guid).sort()).toEqual([
      "FF011000012B467ED8",
      "FF011000012D2FBD46",
      "FF01100001308ED7FA",
      "FF0110000162638666",
      "FF0110000164B7DCE8",
      "FF011000016EAE6204",
    ]);
    expect(rows.every((r) => r.liftedAt === null && r.bannedBy === "seed:0038")).toBe(true);
    // The one stated relationship is recorded; none is invented for the rest.
    expect(rows.find((r) => r.guid === "FF011000016EAE6204")?.altOf).toBe("FF0110000162638666");
    expect(rows.filter((r) => r.altOf !== null)).toHaveLength(1);
    expect(await banFor(env, { guid: "FF011000012B467ED8" })).not.toBeNull();
    expect(await banFor(env, { guid: "FF0110000100000000" })).toBeNull();
  });
});

describe("which identities a ban resolves through", () => {
  it("follows the GUID, the account holding it, and the Steam account behind that", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    await ban(env, GUID);

    expect(await banFor(env, { guid: GUID })).not.toBeNull();
    // Case and padding are not an escape: the locker upper-cases, a server log might not.
    expect(await banFor(env, { guid: " aa0110000100000001 " })).not.toBeNull();
    expect(await banFor(env, { accountId: "acc_banned" })).not.toBeNull();
    expect(await banFor(env, { steamId: BUYER })).not.toBeNull();

    expect(await banFor(env, { accountId: "acc_clean" })).toBeNull();
    expect(await banFor(env, { steamId: CLEAN })).toBeNull();
    expect(await banFor(env, {})).toBeNull();
  });

  it("follows a second account on the same Steam login, and the link log after it is lost", async () => {
    const env = await deployment();
    await account(env, "acc_first", "first-token", BUYER, GUID);
    await ban(env, GUID);

    // The alt: a fresh account, a fresh GUID, the same Steam login.
    await account(env, "acc_alt", "alt-token", null, "FF0110000199999999");
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES (?, ?, ?)")
      .bind("acc_alt", BUYER, Date.now())
      .run();
    expect(await banFor(env, { accountId: "acc_alt", steamId: BUYER })).not.toBeNull();

    // And still, once the first account's `steam_id` column has been lost: the link log is
    // what ties the identity together, exactly as it does for entitlement.
    await env.DB.prepare("UPDATE accounts SET steam_id = NULL WHERE id = 'acc_first'").run();
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES (?, ?, ?)")
      .bind("acc_first", BUYER, Date.now())
      .run();
    expect(await banFor(env, { steamId: BUYER })).not.toBeNull();
  });

  it("cannot move a Steam account's GUID off a ban — it is derived, not chosen", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, null);
    const derived = guidFromSteamId(BUYER)!;
    const claim = (guid: string) =>
      call(env, req("PUT", "/v1/me/guid", { key: "banned-token", body: { guid }, origin: null }));

    // Whatever the app asks for, it gets its own derived GUID: the server never trusts the value.
    expect(await (await claim("FF0110000111111111")).json()).toEqual({ ok: true, guid: derived });
    await ban(env, derived);

    // Once banned, a second claim never even reaches the GUID logic: the estate gate refuses it,
    // disguised. There was nowhere to move to in any case — the GUID is derived, not chosen.
    const moved = await claim("FF0110000122222222");
    expect(moved.status).toBe(403);
    expect(await moved.json()).toMatchObject({ error: APP_BLOCK_MESSAGE });
    expect(await banFor(env, { accountId: "acc_banned" })).not.toBeNull();
  });

  it("a non-Steam account's ban outlives a GUID change, through the claim log", async () => {
    const env = await deployment();
    // A Piboso copy: no SteamID64, so the GUID is opaque and first-come.
    await account(env, "acc_piboso", "piboso-token", null, null);
    const claim = (guid: string) =>
      call(env, req("PUT", "/v1/me/guid", { key: "piboso-token", body: { guid }, origin: null }));
    const first = "AA0110000100000010";
    const other = "AA0110000100000020";

    expect((await claim(first)).status).toBe(200);
    await ban(env, first);

    // Now caught, and a rename cannot escape: the gate refuses the next claim, and the claim log
    // keeps the account tied to the GUID it was banned on either way.
    expect((await claim(other)).status).toBe(403);
    expect(await banFor(env, { accountId: "acc_piboso" })).not.toBeNull();
  });

  it("follows a GUID a diagnostics report named, even one the column never held", async () => {
    const env = await deployment();
    await account(env, "acc_rider", "rider-token", BUYER, OTHER_GUID);
    // The second install on one account — the case `accounts.guid` cannot hold at all.
    await rememberGuid(env, "acc_rider", GUID);
    await ban(env, GUID);
    expect(await banFor(env, { accountId: "acc_rider" })).not.toBeNull();
    expect(await banFor(env, { steamId: BUYER })).not.toBeNull();
  });

  it("follows the Steam login of the calling account, not only the GUID on it", async () => {
    // The commonest caller of all: a bearer token, so the only identity to hand is an account id.
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    // `accounts.steam_id` is unique, so a second account on one Steam login carries the link in
    // the log rather than the column — which is how this actually looks here.
    await account(env, "acc_alt", "alt-token", null, null);
    await ban(env, GUID);
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES (?, ?, ?)")
      .bind("acc_alt", BUYER, Date.now())
      .run();
    expect(await banFor(env, { accountId: "acc_alt" })).not.toBeNull();
  });

  it("stops resolving the moment the ban is lifted, and keeps the row", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await ban(env, GUID);
    expect(await liftBan(env, GUID, BOSS, "appeal upheld")).toEqual({ ok: true, guid: GUID });

    expect(await banFor(env, { steamId: BUYER })).toBeNull();
    const row = (await listBans(env)).find((r) => r.guid === GUID)!;
    expect(row.liftedBy).toBe(BOSS);
    expect(row.liftedNote).toBe("appeal upheld");
    // And the accounts behind it are still readable, which is what makes the page reviewable.
    expect(row.accounts).toEqual([{ accountId: "acc_banned", riderName: "acc_banned", steamId: BUYER, current: true }]);
    expect(await liftBan(env, GUID, BOSS)).toEqual({ ok: false, error: "that GUID isn't banned" });
  });
});

describe("what a ban actually refuses", () => {
  it("refuses the key grant for content the buyer owns, and writes the refusal down", async () => {
    const env = await deployment();
    await account(env, "acc_buyer", "buyer-token", BUYER, GUID);
    const assetId = await ownedAsset(env, BUYER);
    const grant = () =>
      call(env, req("POST", "/v1/keys/grant", { key: "buyer-token", body: { assetId, sessionId: "s1" }, origin: null }));

    expect((await grant()).status).toBe(200);

    await ban(env, GUID);
    const refused = await grant();
    expect(refused.status).toBe(403);
    // Disguised: the app is never told it is a ban.
    expect(await refused.json()).toEqual({ error: APP_BLOCK_MESSAGE });

    // The ledger's own word, so a banned install sweeping the catalogue is visible in it.
    const log = await env.DB.prepare(
      "SELECT decision, reason FROM entitlement_grants WHERE steam_id = ? ORDER BY id DESC LIMIT 1",
    )
      .bind(BUYER)
      .first();
    expect(log).toEqual({ decision: "deny", reason: "banned" });

    // An asset id nobody registered is still the unlogged answer it was: a banned caller is
    // adversarial by definition, and must not be the one account that can write ledger rows by
    // making ids up.
    const unknown = await call(
      env,
      req("POST", "/v1/keys/grant", { key: "buyer-token", body: { assetId: "trk_nope", sessionId: "s9" }, origin: null }),
    );
    expect(await unknown.json()).toEqual({ error: "no such asset" });
    expect(
      await env.DB.prepare("SELECT COUNT(*) AS n FROM entitlement_grants WHERE asset_id = 'trk_nope'").first(),
    ).toEqual({ n: 0 });

    // And the check that the app asks first agrees with the grant, because it is the same call.
    const check = await call(
      env,
      req("POST", "/v1/entitlements/check", { key: "buyer-token", body: { assetId, sessionId: "s2" }, origin: null }),
    );
    expect(check.status).toBe(403);
    expect(await check.json()).toEqual({ allowed: false, reason: "unavailable" });

    // Lifting it puts the buyer back where they were: the entitlement was never touched.
    await liftBan(env, GUID, BOSS);
    expect((await grant()).status).toBe(200);
  });

  it("tells the app to delete the keys already on the machine", async () => {
    const env = await deployment();
    await account(env, "acc_buyer", "buyer-token", BUYER, GUID);
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    const assetId = await ownedAsset(env, BUYER);
    await env.DB.prepare("INSERT INTO entitlements (steam_id, asset_id, source, granted_at) VALUES (?, ?, 'grant', ?)")
      .bind(CLEAN, assetId, Date.now())
      .run();
    const status = (token: string) =>
      call(env, req("POST", "/v1/assets/status", { key: token, body: { assetIds: [assetId] }, origin: null }));
    const asset = async (token: string) =>
      ((await (await status(token)).json()) as { assets: Record<string, unknown>[] }).assets[0];

    expect(await asset("buyer-token")).toMatchObject({ owned: true, available: true, revoked: false });

    await ban(env, GUID);
    // Owned stays true — they did buy it, and the creator's records must not start lying — but
    // it is not available and the key on disk goes.
    expect(await asset("buyer-token")).toMatchObject({ owned: true, available: false, revoked: true });
    // Nobody else's machine hears anything about it.
    expect(await asset("clean-token")).toMatchObject({ owned: true, available: true, revoked: false });

    // The entitlement list is simply refused, like everything else the gate covers: the app
    // learns what is going on from `/v1/me`, not from a list it can do nothing with.
    const mine = await call(env, req("GET", "/v1/entitlements", { key: "buyer-token", origin: null }));
    expect(mine.status).toBe(403);
  });

  it("refuses the paid plugins mxbsecure sells, without spending a key or touching a license", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await publishBundle(env.DB, "replaycam", "1.0.0", "abc123");
    const code = await mintOne(env);
    const redeem = () =>
      call(env, req("POST", "/v1/plugins/redeem", { key: "banned-token", body: { code }, origin: null }));
    const mine = () => call(env, req("GET", "/v1/me/plugins", { key: "banned-token", origin: null }));
    const bundle = () => call(env, req("GET", "/v1/plugins/replaycam/bundle", { key: "banned-token", origin: null }));

    expect((await redeem()).status).toBe(200);
    expect((await mine()).status).toBe(200);
    expect((await bundle()).status).toBe(200);

    await ban(env, GUID);
    for (const res of [await mine(), await bundle()]) {
      expect(res.status).toBe(403);
      expect(await res.json()).toMatchObject({ error: APP_BLOCK_MESSAGE });
    }
    // The license row is untouched, so lifting the ban restores exactly what they had.
    expect(
      await env.DB.prepare("SELECT revoked_at FROM plugin_licenses WHERE account_id = 'acc_banned'").first(),
    ).toEqual({ revoked_at: null });
    await liftBan(env, GUID, BOSS);
    expect((await mine()).status).toBe(200);

    // And a key is never spent by a banned account: refused before the code is read.
    const second = await mintOne(env);
    await ban(env, GUID);
    expect((await call(env, req("POST", "/v1/plugins/redeem", { key: "banned-token", body: { code: second }, origin: null }))).status).toBe(403);
    expect(await env.DB.prepare("SELECT redeemed_by FROM plugin_keys WHERE code = ?").bind(second).first()).toEqual({
      redeemed_by: null,
    });
  });

  it("refuses the rest of the estate too — voice, paint sync, presence, the queue, servers", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    // An invited account, so the server-estate routes below are refused for the ban rather than
    // for the invite gate that normally stops a self-serve one.
    await env.DB.prepare("UPDATE accounts SET kind = 'invited' WHERE id = 'acc_banned'").run();
    await ban(env, GUID);
    const as = (method: string, path: string, body?: unknown) =>
      call(env, req(method, path, { key: "banned-token", body, origin: null }));

    // Every app in the brand comes through these, and none of them is about locked content.
    const routes: [string, string, unknown?][] = [
      ["PUT", "/v1/me/name", { riderName: "Someone" }],
      ["PUT", "/v1/presence", { server: "srv_1" }],
      ["GET", "/v1/voice/ice"],
      ["GET", "/v1/voice/room?server=srv_1"],
      ["PUT", "/v1/loadouts", { bikes: [] }],
      ["GET", "/v1/presence?server=srv_1"],
      ["PUT", "/v1/queue", { server: "srv_1" }],
      ["GET", "/v1/me/plugins"],
      ["GET", "/v1/entitlements"],
      ["POST", "/v1/servers", { name: "Frost", address: "1.2.3.4:54000" }],
      ["GET", "/v1/servers/mine"],
      ["POST", "/v1/provision", { region: "eu-west-1" }],
      ["GET", "/v1/fleet"],
    ];
    for (const [method, path, body] of routes) {
      const res = await as(method, path, body);
      expect(res.status, `${method} ${path}`).toBe(403);
      // Every app-facing refusal wears the same disguise — never the word "ban".
      expect(await res.json()).toMatchObject({ error: APP_BLOCK_MESSAGE });
    }

    // `GET /v1/roster` is not in the list: the public server book answers that path for
    // everybody before the account gate is reached (the bearer paint roster below it is
    // shadowed by it, which predates this and is not a ban's business). Publishing a look is
    // gated, which is the half of paint sync a ban is actually about.

    // Nothing is refused for an account that isn't banned, on any of them: the gate is about
    // who is asking, not about the routes.
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    const clean = await call(env, req("GET", "/v1/voice/ice", { key: "clean-token", origin: null }));
    expect(clean.status).toBe(200);
  });

  it("still answers the four things a ban needs to stay reachable", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await ban(env, GUID);
    const as = (method: string, path: string, body?: unknown) =>
      call(env, req(method, path, { key: "banned-token", body, origin: null }));

    // Who am I: still answers, and still looks ordinary — the app is never told here that it
    // is banned. `/v1/app/gate` is what turns it away, with a reason that is not the truth.
    const me = await as("GET", "/v1/me");
    expect(me.status).toBe(200);
    const body = (await me.json()) as Record<string, unknown>;
    expect(body).toMatchObject({ steamId: BUYER });
    expect(body.banned).toBeUndefined();
    expect(body.banReason).toBeUndefined();

    // The gate: a banned install is told to stand down, mundanely, with no mention of a ban.
    const gate = await as("GET", "/v1/app/gate");
    expect(gate.status).toBe(200);
    const verdict = (await gate.json()) as Record<string, unknown>;
    expect(verdict.status).toBe("unsupported");
    expect(String(verdict.message)).not.toMatch(/ban/i);
    expect(verdict.message).toBe(APP_BLOCK_MESSAGE);

    // Diagnostics still observes, and still says nothing about what it made of the report.
    const report = await as("PUT", "/v1/diagnostics", { available: false, appVersion: "0.15.1", guid: GUID });
    expect(report.status).toBe(200);
    expect(await report.json()).toEqual({ ok: true });

    // And the status poll still answers, because a 403 there would keep the keys on disk. It
    // carries no ban flag — the per-asset `revoked` does the work, and reads as an ordinary
    // removal.
    const status = await as("POST", "/v1/assets/status", { assetIds: [] });
    expect(status.status).toBe(200);
    expect(((await status.json()) as Record<string, unknown>).banned).toBeUndefined();
  });

  it("closes the site: no creator signup, no locker, and /me says so", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await env.DB.prepare("UPDATE accounts SET creator_at = 1, creator_source = 'self' WHERE id = 'acc_banned'").run();
    const who = await cookie(BUYER);
    await ban(env, GUID);

    const me = await call(env, req("GET", "/v1/web/me", { cookie: who }));
    expect(await me.json()).toMatchObject({
      steamId: BUYER,
      banned: true,
      banReason: "unlocked and shared protected content",
      // Their creator standing is untouched in the database; the site is simply closed to them.
      creator: false,
    });
    expect(await env.DB.prepare("SELECT creator_at FROM accounts WHERE id = 'acc_banned'").first()).toEqual({ creator_at: 1 });

    const signup = await call(env, req("POST", "/v1/web/creator", { cookie: who, body: {} }));
    expect(signup.status).toBe(403);
    const locker = await call(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js", { cookie: who }));
    expect(locker.status).toBe(403);
    // The dashboard behind it too: nothing new gets locked or granted.
    const dashboard = await call(env, req("GET", "/admin/assets", { cookie: who }));
    expect(dashboard.status).toBe(403);
    expect(await dashboard.json()).toEqual({ error: "this install is banned from mxbsecure" });

    // A creator API key belonging to the same account is just as dead.
    await env.DB.prepare(
      "INSERT INTO creator_keys (id, account_id, label, token_hash, created_at) VALUES ('ck_1', 'acc_banned', 'shop', ?, ?)",
    )
      .bind(await hashToken(CREATOR_KEY), Date.now())
      .run();
    const byKey = await call(env, req("GET", "/admin/assets", { key: CREATOR_KEY, origin: null }));
    expect(byKey.status).toBe(403);
  });

  it("leaves a clean creator alone", async () => {
    const env = await deployment();
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    await env.DB.prepare("UPDATE accounts SET creator_at = 1, creator_source = 'self' WHERE id = 'acc_clean'").run();
    await ban(env, GUID);
    const who = await cookie(CLEAN);
    const me = (await (await call(env, req("GET", "/v1/web/me", { cookie: who }))).json()) as Record<string, unknown>;
    expect(me).toMatchObject({ creator: true });
    expect(me.banned).toBeUndefined();
    expect((await call(env, req("GET", "/v1/web/lockweb/mxb_lockweb.js", { cookie: who }))).status).toBe(200);
    expect((await call(env, req("GET", "/admin/assets", { cookie: who }))).status).toBe(200);
  });
});

describe("the desktop apps' startup gate", () => {
  it("tells a clean install ok, and a banned one a mundane untruth", async () => {
    const env = await deployment();
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    await account(env, "acc_banned", "banned-token", BUYER, GUID);

    expect(await appGate(env, { accountId: "acc_clean" })).toEqual({ status: "ok" });

    await ban(env, GUID);
    const verdict = await appGate(env, { accountId: "acc_banned" });
    expect(verdict.status).toBe("unsupported");
    // The whole point: we know it is a ban, the message does not say so.
    expect("message" in verdict && verdict.message).toBe(APP_BLOCK_MESSAGE);
    expect(JSON.stringify(verdict)).not.toMatch(/ban/i);
  });

  it("is reachable through the router by a banned install, and the clean install runs", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    const gate = (token: string) => call(env, req("GET", "/v1/app/gate", { key: token, origin: null }));

    expect(await (await gate("clean-token")).json()).toEqual({ status: "ok" });
    expect(await (await gate("banned-token")).json()).toEqual({ status: "ok" });

    await ban(env, GUID);
    // The banned install still reaches the gate (it is on the allow-list) — that is how it is
    // told to stop, rather than getting a bare 403 it would read as an outage.
    const blocked = await gate("banned-token");
    expect(blocked.status).toBe(200);
    expect(await blocked.json()).toEqual({ status: "unsupported", message: APP_BLOCK_MESSAGE });
    // And a clean install is unaffected.
    expect(await (await gate("clean-token")).json()).toEqual({ status: "ok" });
  });
});

describe("writing the list down", () => {
  it("records who banned, refuses a ban with nothing said, and never double-dates one", async () => {
    const env = await deployment();
    expect(await addBan(env, { guid: "nope!", reason: "x" }, BOSS)).toEqual({
      ok: false,
      error: "that doesn't look like an MX Bikes GUID",
    });
    expect(await addBan(env, { guid: GUID, reason: "  " }, BOSS)).toEqual({ ok: false, error: "say why, in a few words" });
    expect(await addBan(env, { guid: GUID, reason: "shared unlocked content", altOf: GUID }, BOSS)).toEqual({
      ok: false,
      error: "a GUID cannot be an alt of itself",
    });

    const first = await addBan(
      env,
      { guid: ` ${GUID.toLowerCase()} `, reason: "shared unlocked content", evidence: "discord thread", altOf: OTHER_GUID },
      BOSS,
    );
    expect(first).toEqual({ ok: true, guid: GUID, already: false });
    const row = (await listBans(env)).find((r) => r.guid === GUID)!;
    expect(row).toMatchObject({ reason: "shared unlocked content", evidence: "discord thread", altOf: OTHER_GUID, bannedBy: BOSS });

    // A reload and a second click must not rewrite the reason it was first applied under.
    expect(await addBan(env, { guid: GUID, reason: "something else" }, "76561198000000001")).toEqual({
      ok: true,
      guid: GUID,
      already: true,
    });
    expect((await listBans(env)).find((r) => r.guid === GUID)).toMatchObject({ reason: "shared unlocked content", bannedBy: BOSS });

    // A lifted one is re-banned in place, with the new reason and the admin who did it.
    await liftBan(env, GUID, BOSS, "appeal");
    expect(await addBan(env, { guid: GUID, reason: "caught again" }, BOSS)).toEqual({ ok: true, guid: GUID, already: false });
    expect((await listBans(env)).find((r) => r.guid === GUID)).toMatchObject({
      reason: "caught again",
      liftedAt: null,
      liftedNote: null,
    });
  });

  it("is only the admins' to write, through the dashboard endpoint", async () => {
    const env = await deployment();
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    const boss = await cookie(BOSS);
    const post = (body: unknown, who: string) => call(env, req("POST", "/v1/web/admin/bans", { cookie: who, body }));

    expect((await post({ action: "ban", guid: GUID, reason: "shared it" }, await cookie(CLEAN))).status).toBe(403);
    expect((await call(env, req("GET", "/v1/web/admin/bans", { cookie: await cookie(CLEAN) }))).status).toBe(403);

    const added = await post({ action: "ban", guid: GUID, reason: "shared it", evidence: "clip" }, boss);
    expect(added.status).toBe(200);
    expect(await banFor(env, { steamId: BUYER })).not.toBeNull();

    const listed = await call(env, req("GET", "/v1/web/admin/bans", { cookie: boss }));
    const { bans } = (await listed.json()) as { bans: { guid: string; bannedBy: string; accounts: unknown[] }[] };
    const mine = bans.find((b) => b.guid === GUID)!;
    expect(mine.bannedBy).toBe(BOSS);
    expect(mine.accounts).toHaveLength(1);

    expect((await post({ action: "lift", guid: GUID, note: "wrong install" }, boss)).status).toBe(200);
    expect(await banFor(env, { steamId: BUYER })).toBeNull();
    expect((await post({ action: "lift", guid: GUID }, boss)).status).toBe(404);
    expect((await post({ action: "sideways", guid: GUID }, boss)).status).toBe(400);
  });

  it("normalises a GUID the way the locker does, and refuses what isn't one", () => {
    expect(normalizeGuid(" aa0110000100000001 ")).toBe(GUID);
    expect(normalizeGuid(GUID)).toBe(GUID);
    expect(normalizeGuid("has a space")).toBeNull();
    expect(normalizeGuid("")).toBeNull();
    expect(normalizeGuid(42)).toBeNull();
  });
});
