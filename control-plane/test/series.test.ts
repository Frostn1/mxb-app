import { describe, expect, it, vi } from "vitest";
import { addBan } from "../src/bans";
import { adminListSeries, adminSeriesAction, findIdentifier, nextRound, parsePublish, pruneSeriesRegistrations } from "../src/series";
import { guidFromSteamId } from "../src/steam";
import { SESSION_COOKIE, sealToken } from "../src/websession";
import { d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const API = "https://cp.test";
const SITE = "https://racing.mxbsecure.com";
const KEY = "series-test-session-key-series-test";
// Made-up identifiers in the shapes the endpoints must handle. Not anybody's.
const FAKE_STEAM_ID = "76561190000000001";
const SIGNED_IN_STEAM = "76561202160265728"; // account 4,200,000,000: far past any issued account
const GUID_A = "FF0110000000000A01";
const GUID_B = "FF0110000000000B02";

function env(): Env {
  return { DB: d1(), MXB_WEB_SESSION_KEY: KEY } as unknown as Env;
}

const ctx = { waitUntil() {}, passThroughOnException() {} } as unknown as ExecutionContext;
const call = (e: Env, path: string, init: RequestInit = {}) => worker.fetch(new Request(`${API}${path}`, init), e, ctx);

async function reserve(e: Env, slug = "winter-26", name = "Winter Series"): Promise<string> {
  const r = await adminSeriesAction(e, { action: "create", slug, name });
  expect(r.status).toBe(200);
  return (r.body as { token: string }).token;
}

function body(overrides: Record<string, unknown> = {}) {
  return {
    v: 1,
    name: "Winter Series",
    classes: ["MX1"],
    points_table: [25, 22, 20],
    drop_worst: 0,
    upcoming: [{ label: "Round 3", track: "Club MX", starts_at: 4_000_000_000 }],
    rounds: [
      {
        round: 1,
        label: "Round 1",
        track: "Club MX",
        started_unix: 1_759_000_000,
        results: [
          { place: 1, name: "Rider A", guid: GUID_A, number: 7, class: "MX1", points: 25, status: "finished", laps: 12, best_lap_ms: 98_123 },
          { place: 2, name: "Rider B", guid: GUID_B, class: "MX1", points: 22, status: "finished", laps: 12, best_lap_ms: null },
          { place: null, name: "Rider C", guid: null, class: "MX1", points: 0, status: "dnf", laps: 3, best_lap_ms: null },
        ],
      },
      { round: 2, label: "Round 2", track: "Mud Pit", started_unix: 1_759_600_000, status: "dropped", results: [] },
    ],
    standings: [
      { position: 2, name: "Rider B", guid: GUID_B, class: "MX1", points: 22, gross_points: 22, wins: 0, rounds_ridden: 1, rounds: [{ round: 1, place: 2, points: 22, dropped: false }] },
      { position: 1, name: "Rider A", guid: GUID_A, number: 7, class: "MX1", points: 25, gross_points: 25, wins: 1, rounds_ridden: 1, rounds: [{ round: 1, place: 1, points: 25, dropped: false }] },
    ],
    ...overrides,
  };
}

const put = (e: Env, slug: string, token: string, payload: unknown) =>
  call(e, `/v1/series/${slug}`, {
    method: "PUT",
    headers: { Authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(payload),
  });

const registerReq = (e: Env, slug: string, payload: Record<string, unknown>, ip = "203.0.113.5") =>
  call(e, `/v1/series/${slug}/register`, {
    method: "POST",
    headers: { "content-type": "application/json", "CF-Connecting-IP": ip },
    body: JSON.stringify(payload),
  });

async function sessionCookie(steamId = SIGNED_IN_STEAM): Promise<string> {
  return `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId, name: "Signed In", exp: Date.now() + 60_000 }, KEY)}`;
}

describe("identifier screening", () => {
  it("allows a GUID only in a rider's guid field", () => {
    expect(findIdentifier({ a: [{ name: `x ${FAKE_STEAM_ID}` }] })).toBe("body.a[0].name");
    expect(findIdentifier({ name: GUID_A })).toBe("body.name");
    expect(findIdentifier({ team: "123e4567-e89b-12d3-a456-426614174000" })).toBe("body.team");
    expect(findIdentifier({ rows: [{ key: "name:rider a" }] })).toBe("body.rows[0].key");
    expect(findIdentifier({ rows: [{ steamId: "x" }] })).toBe("body.rows[0].steamId");
    expect(findIdentifier({ rows: [{ guid: GUID_A }] })).toBeNull();
    expect(findIdentifier(body())).toBeNull();
  });

  it("refuses identifiers elsewhere and malformed guids", () => {
    const withGuidName = body();
    (withGuidName.rounds[0].results[0] as Record<string, unknown>).name = GUID_B;
    expect(parsePublish(withGuidName)).toMatchObject({ error: expect.stringContaining("GUID") });
    const withKey = body({ standings: [{ position: 1, name: "A", points: 1, key: "abc" }] });
    expect(parsePublish(withKey)).toMatchObject({ error: expect.stringContaining("key") });
    const badGuid = body({ standings: [{ position: 1, name: "A", points: 1, guid: "has spaces in it" }] });
    expect(parsePublish(badGuid)).toMatchObject({ error: expect.stringContaining("guid") });
  });

  it("validates shape, version and round status; takes v1 next_round", () => {
    expect(parsePublish({ ...body(), v: 2 })).toMatchObject({ error: expect.any(String) });
    expect(parsePublish(body({ name: "" }))).toMatchObject({ error: expect.any(String) });
    expect(parsePublish(body({ rounds: [{ round: 1, status: "maybe", results: [] }] }))).toMatchObject({ error: expect.stringContaining("status") });
    const ok = parsePublish(body());
    expect("error" in ok).toBe(false);
    if (!("error" in ok)) {
      expect(ok.standings.map((s) => s.name)).toEqual(["Rider A", "Rider B"]);
      expect(ok.rounds.map((r) => r.status)).toEqual(["done", "dropped"]);
    }
    const legacy = parsePublish(body({ upcoming: undefined, next_round: { label: "R9", starts_at: 5 } }));
    expect(legacy).toMatchObject({ upcoming: [{ label: "R9", track: null, startsAt: 5 }] });
  });

  it("picks the next round still ahead", () => {
    const up = [{ label: "a", track: null, startsAt: 100 }, { label: "b", track: null, startsAt: 100_000 }];
    expect(nextRound(up, 50_000)?.label).toBe("b");
    expect(nextRound(up, 200_000)).toBeNull();
    expect(nextRound([{ label: "tbd", track: null, startsAt: null }], 1)?.label).toBe("tbd");
  });
});

describe("admin", () => {
  it("reserves a slug once, rotates, lists", async () => {
    const e = env();
    const first = await reserve(e);
    expect(first.length).toBeGreaterThan(20);
    expect((await adminSeriesAction(e, { action: "create", slug: "winter-26", name: "Again" })).status).toBe(409);
    expect((await adminSeriesAction(e, { action: "create", slug: "Bad Slug", name: "x" })).status).toBe(400);
    const stored = await e.DB.prepare("SELECT token_hash FROM series").first<{ token_hash: string }>();
    expect(stored!.token_hash).not.toBe(first);

    const rotated = await adminSeriesAction(e, { action: "rotate", slug: "winter-26" });
    const second = (rotated.body as { token: string }).token;
    expect(second).not.toBe(first);
    expect((await put(e, "winter-26", first, body())).status).toBe(401);
    expect((await put(e, "winter-26", second, body())).status).toBe(200);

    const list = await adminListSeries(e);
    expect(list.body).toMatchObject({ series: [{ slug: "winter-26", published: true, pending: 0, approved: 0 }] });
  });
});

describe("publish and public reads", () => {
  it("is invisible until published, then readable with CORS and without GUIDs", async () => {
    const e = env();
    const token = await reserve(e);
    expect((await call(e, "/v1/series/winter-26")).status).toBe(404);
    expect(await (await call(e, "/v1/series")).json()).toEqual({ series: [] });

    const res = await put(e, "winter-26", token, body());
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ ok: true, url: "https://mxbsecure.com/series/winter-26" });

    // Stored privately...
    const stored = await e.DB.prepare("SELECT standings FROM series_standings").first<{ standings: string }>();
    expect(stored!.standings).toContain(GUID_A);

    const read = await call(e, "/v1/series/winter-26");
    expect(read.status).toBe(200);
    expect(read.headers.get("access-control-allow-origin")).toBe("*");
    const raw = await read.text();
    // ...never served.
    expect(raw).not.toContain(GUID_A);
    expect(raw).not.toContain(GUID_B);
    expect(raw).not.toMatch(/guid/i);
    expect(raw).not.toMatch(/mmrNote|approximate/);
    const s = JSON.parse(raw) as Record<string, any>;
    expect(s.name).toBe("Winter Series");
    expect(s.nextRound).toEqual({ label: "Round 3", track: "Club MX", startsAt: 4_000_000_000 });
    expect(s.upcoming).toHaveLength(1);
    expect(s.standings[0]).toEqual({
      position: 1, number: 7, name: "Rider A", class: "MX1", points: 25, grossPoints: 25, wins: 1, roundsRidden: 1,
      rounds: [{ round: 1, place: 1, points: 25, dropped: false }], mmr: null,
    });
    expect(s.rounds.map((r: any) => r.status)).toEqual(["done", "dropped"]);
    expect(s.rounds[0].results[0]).toEqual({ place: 1, name: "Rider A", number: 7, class: "MX1", points: 25, status: "finished", laps: 12, bestLapMs: 98_123 });

    const list = await (await call(e, "/v1/series")).text();
    expect(list).not.toContain(GUID_A);
    expect(JSON.parse(list).series[0]).toMatchObject({ slug: "winter-26", rounds: 1, riders: 2, leader: { name: "Rider A", points: 25 } });
  });

  it("rejects bad tokens, other series' tokens and identifier-shaped names", async () => {
    const e = env();
    const a = await reserve(e, "series-a", "A");
    await reserve(e, "series-b", "B");
    expect((await put(e, "series-b", a, body())).status).toBe(401);
    expect((await put(e, "nope", a, body())).status).toBe(404);
    expect((await call(e, "/v1/series/series-a", { method: "PUT", body: "{}" })).status).toBe(401);
    const bad = body();
    (bad.standings[0] as Record<string, unknown>).name = `Rider ${FAKE_STEAM_ID}`;
    expect((await put(e, "series-a", a, bad)).status).toBe(400);
    expect((await call(e, "/v1/series/series-a")).status).toBe(404);
  });

  it("replaces rounds on republish, keeps a few snapshots, and unpublishes", async () => {
    const e = env();
    const token = await reserve(e);
    for (let i = 0; i < 12; i++) await put(e, "winter-26", token, body());
    await put(e, "winter-26", token, body({ rounds: [] }));
    const snaps = await e.DB.prepare("SELECT COUNT(*) AS n FROM series_standings").first<{ n: number }>();
    expect(snaps!.n).toBe(10);
    const s = (await (await call(e, "/v1/series/winter-26")).json()) as { rounds: unknown[] };
    expect(s.rounds).toEqual([]);

    const del = await call(e, "/v1/series/winter-26", { method: "DELETE", headers: { Authorization: `Bearer ${token}` } });
    expect(del.status).toBe(200);
    expect((await call(e, "/v1/series/winter-26")).status).toBe(404);
  });

  it("answers CORS preflight", async () => {
    const e = env();
    const res = await call(e, "/v1/series/winter-26", { method: "OPTIONS" });
    expect(res.status).toBe(204);
    expect(res.headers.get("access-control-allow-methods")).toContain("PUT");
    expect(res.headers.get("access-control-allow-headers")).toContain("Authorization");
  });
});

describe("MMR by GUID", () => {
  async function rate(e: Env, guid: string, cls: string, rating: number, races = 7) {
    await e.DB.prepare(
      "INSERT INTO rider_ratings (guid, class, rating, rd, volatility, races, updated_at) VALUES (?, ?, ?, 50, 0.06, ?, 0)",
    ).bind(guid, cls, rating, races).run();
  }
  const row = (name: string, guid: string | null, cls = "MX1", position = 1) => ({
    position, name, guid, class: cls, points: 10, gross_points: 10, wins: 0, rounds_ridden: 1, rounds: [],
  });

  it("joins on GUID and class, whatever the name; banned and GUID-less riders get none", async () => {
    const e = env();
    const token = await reserve(e);
    await rate(e, GUID_A, "MX1", 1612.4);
    await rate(e, GUID_A, "MX2", 1400);
    await rate(e, "BANNED", "MX1", 1700);
    expect(await addBan(e, { guid: "BANNED", reason: "cracked content" }, "seed:test")).toMatchObject({ ok: true });
    const standings = [row("A different name", GUID_A, "MX1", 1), row("Rider C", "BANNED", "MX1", 2), row("Rider A", null, "MX1", 3)];
    await put(e, "winter-26", token, body({ standings }));
    const s = (await (await call(e, "/v1/series/winter-26")).json()) as { standings: { mmr: unknown }[] };
    expect(s.standings.map((x) => x.mmr)).toEqual([1612, null, null]);
  });

  it("uses the only series class, else the most-raced class, when a row has none", async () => {
    const e = env();
    const token = await reserve(e);
    await rate(e, GUID_A, "MX1", 1612, 3);
    await rate(e, GUID_A, "MX2", 1500, 20);
    await put(e, "winter-26", token, body({ standings: [row("Rider A", GUID_A, "")] }));
    let s = (await (await call(e, "/v1/series/winter-26")).json()) as { standings: { mmr: unknown }[] };
    expect(s.standings[0].mmr).toBe(1612);
    await put(e, "winter-26", token, body({ classes: [], standings: [row("Rider A", GUID_A, "")] }));
    s = (await (await call(e, "/v1/series/winter-26")).json()) as { standings: { mmr: unknown }[] };
    expect(s.standings[0].mmr).toBe(1500);
  });
});

describe("registration", () => {
  async function published(e: Env, overrides: Record<string, unknown> = {}) {
    const token = await reserve(e);
    await put(e, "winter-26", token, body(overrides));
    return token;
  }
  const entry = { name: "New Rider", number: 41, class: "MX1", team: "Garage", discord: "newrider", website: "" };
  const auth = (token: string) => ({ Authorization: `Bearer ${token}`, "content-type": "application/json" });

  it("anonymous: pending and unverified; operator links a GUID and approves; public shows no Discord or GUID", async () => {
    const e = env();
    const token = await published(e);
    const res = await registerReq(e, "winter-26", entry);
    expect(res.status).toBe(201);
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
    expect(await res.json()).toEqual({ ok: true, status: "pending", verified: false });

    let s = (await (await call(e, "/v1/series/winter-26")).json()) as { entries: unknown[] };
    expect(s.entries).toEqual([]);

    expect((await call(e, "/v1/series/winter-26/registrations")).status).toBe(401);
    const list = (await (await call(e, "/v1/series/winter-26/registrations", { headers: auth(token) })).json()) as {
      registrations: { id: string; discord: string; status: string; verified: boolean; guid: string | null }[];
    };
    expect(list.registrations[0]).toMatchObject({ discord: "newrider", status: "pending", verified: false, guid: null });

    const id = list.registrations[0].id;
    const decide = await call(e, `/v1/series/winter-26/registrations/${id}`, {
      method: "POST",
      headers: auth(token),
      body: JSON.stringify({ status: "approved", guid: GUID_B.toLowerCase() }),
    });
    expect(await decide.json()).toEqual({ ok: true, id, status: "approved", guid: GUID_B });

    const raw = await (await call(e, "/v1/series/winter-26")).text();
    expect(raw).not.toContain("newrider");
    expect(raw).not.toContain(GUID_B);
    s = JSON.parse(raw);
    expect(s.entries).toEqual([{ name: "New Rider", number: 41, class: "MX1", team: "Garage", verified: true }]);
    // The linked entry's number fills in the standing row that had none.
    expect((s as any).standings.find((r: any) => r.name === "Rider B").number).toBe(41);
  });

  it("signed in: the GUID comes from the Steam session, and the entry is verified", async () => {
    const e = env();
    const token = await published(e);
    const post = async (cookie: string | null, payload = entry) =>
      call(e, "/v1/web/series/winter-26/register", {
        method: "POST",
        headers: { Origin: SITE, "content-type": "application/json", ...(cookie ? { Cookie: cookie } : {}) },
        body: JSON.stringify({ ...payload, guid: GUID_A }),
      });
    expect((await post(null)).status).toBe(401);
    const cookie = await sessionCookie();
    const res = await post(cookie);
    expect(res.status).toBe(201);
    expect(res.headers.get("access-control-allow-origin")).toBe(SITE);
    expect(await res.json()).toEqual({ ok: true, status: "pending", verified: true });
    // A second entry from the same rider, under another name, is the same rider.
    expect((await post(cookie, { ...entry, name: "Alt Name" })).status).toBe(409);

    const mine = await call(e, "/v1/web/series/winter-26/registration", { headers: { Origin: SITE, Cookie: cookie } });
    expect(await mine.json()).toEqual({ entry: { name: "New Rider", number: 41, class: "MX1", team: "Garage", status: "pending" } });

    const list = (await (await call(e, "/v1/series/winter-26/registrations", { headers: auth(token) })).json()) as {
      registrations: { id: string; verified: boolean; guid: string }[];
    };
    // The form's own guid was ignored.
    expect(list.registrations[0]).toMatchObject({ verified: true, guid: guidFromSteamId(SIGNED_IN_STEAM) });
    // And the operator cannot repoint it.
    const relink = await call(e, `/v1/series/winter-26/registrations/${list.registrations[0].id}`, {
      method: "POST", headers: auth(token), body: JSON.stringify({ guid: GUID_B }),
    });
    expect(relink.status).toBe(409);

    // Cross-site writes are refused.
    const offsite = await call(e, "/v1/web/series/winter-26/register", {
      method: "POST",
      headers: { Origin: "https://evil.example", "content-type": "application/json", Cookie: cookie },
      body: JSON.stringify(entry),
    });
    expect(offsite.status).toBe(403);
  });

  it("refuses a banned signed-in rider", async () => {
    const e = env();
    await published(e);
    expect(await addBan(e, { guid: guidFromSteamId(SIGNED_IN_STEAM), reason: "cracked content" }, "seed:test")).toMatchObject({ ok: true });
    const res = await call(e, "/v1/web/series/winter-26/register", {
      method: "POST",
      headers: { Origin: SITE, "content-type": "application/json", Cookie: await sessionCookie() },
      body: JSON.stringify(entry),
    });
    expect(res.status).toBe(403);
  });

  it("refuses duplicates, bad classes, closed series and IDs; swallows the honeypot", async () => {
    const e = env();
    await published(e);
    expect((await registerReq(e, "winter-26", entry)).status).toBe(201);
    expect((await registerReq(e, "winter-26", { ...entry, name: " new rider " })).status).toBe(409);
    expect((await registerReq(e, "winter-26", { ...entry, name: "Other", class: "MX3" })).status).toBe(400);
    expect((await registerReq(e, "winter-26", { ...entry, name: FAKE_STEAM_ID })).status).toBe(400);
    expect((await registerReq(e, "winter-26", { ...entry, name: "Other", team: GUID_A })).status).toBe(400);
    expect((await registerReq(e, "winter-26", { ...entry, name: "Other", number: 1000 })).status).toBe(400);
    expect((await registerReq(e, "missing", entry)).status).toBe(404);

    const trap = await registerReq(e, "winter-26", { ...entry, name: "Bot", website: "http://spam" });
    expect(trap.status).toBe(201);
    const n = await e.DB.prepare("SELECT COUNT(*) AS n FROM series_registrations").first<{ n: number }>();
    expect(n!.n).toBe(1);

    const closed = env();
    await published(closed, { registration_open: false });
    expect((await registerReq(closed, "winter-26", entry)).status).toBe(403);
  });

  it("caps registrations per address per day and forgets the address later", async () => {
    const e = env();
    await published(e);
    for (let i = 0; i < 10; i++) expect((await registerReq(e, "winter-26", { ...entry, name: `Rider ${i}` })).status).toBe(201);
    expect((await registerReq(e, "winter-26", { ...entry, name: "Rider 11" })).status).toBe(429);
    expect((await registerReq(e, "winter-26", { ...entry, name: "Rider 11" }, "198.51.100.7")).status).toBe(201);

    await pruneSeriesRegistrations(e, Date.now() + 3 * 24 * 60 * 60 * 1000);
    const left = await e.DB.prepare("SELECT COUNT(*) AS n FROM series_registrations WHERE ip_hash IS NOT NULL").first<{ n: number }>();
    expect(left!.n).toBe(0);
  });

  it("uses the limiter binding when there is one", async () => {
    const e = env();
    await published(e);
    (e as unknown as { REGISTER_LIMITER: RateLimit }).REGISTER_LIMITER = { limit: async () => ({ success: false }) } as unknown as RateLimit;
    expect((await registerReq(e, "winter-26", entry)).status).toBe(429);
  });
});

describe("rating leaderboard for the site", () => {
  it("is CORS-open and lists rated classes", async () => {
    const e = env();
    await e.DB.prepare("INSERT INTO rider_ratings (guid, class, rating, rd, volatility, races, updated_at) VALUES ('g', 'MX2', 1500, 50, 0.06, 1, 0)").run();
    const lb = await call(e, "/v1/rating/leaderboard?class=MX2");
    expect(lb.headers.get("access-control-allow-origin")).toBe("*");
    const classes = await call(e, "/v1/rating/classes");
    expect(await classes.json()).toEqual({ classes: ["MX2"] });
  });
});
