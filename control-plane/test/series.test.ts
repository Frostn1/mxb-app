import { describe, expect, it, vi } from "vitest";
import { addBan } from "../src/bans";
import { adminListSeries, adminSeriesAction, findIdentifier, parsePublish, pruneSeriesRegistrations } from "../src/series";
import { d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const API = "https://cp.test";
// Made-up identifiers in the shapes the publish endpoint must refuse. Not anybody's.
const FAKE_STEAM_ID = "76561190000000001";
const FAKE_GUID = "FF0110000100000000";

function env(): Env {
  return { DB: d1() } as unknown as Env;
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
    next_round: { label: "Round 3", track: "Club MX", starts_at: 1_760_000_000 },
    rounds: [
      {
        round: 1,
        label: "Weekend",
        track: "Club MX",
        started_unix: 1_759_000_000,
        results: [
          { place: 1, name: "Rider A", class: "MX1", points: 25, status: "finished", laps: 12, best_lap_ms: 98_123 },
          { place: 2, name: "Rider B", class: "MX1", points: 22, status: "finished", laps: 12, best_lap_ms: null },
          { place: null, name: "Rider C", class: "MX1", points: 0, status: "dnf", laps: 3, best_lap_ms: null },
        ],
      },
    ],
    standings: [
      { position: 2, name: "Rider B", class: "MX1", points: 22, gross_points: 22, wins: 0, rounds_ridden: 1, rounds: [{ round: 1, place: 2, points: 22, dropped: false }] },
      { position: 1, name: "Rider A", class: "MX1", points: 25, gross_points: 25, wins: 1, rounds_ridden: 1, rounds: [{ round: 1, place: 1, points: 25, dropped: false }] },
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

describe("identifier screening", () => {
  it("finds Steam IDs, GUIDs, UUIDs and forbidden keys anywhere", () => {
    expect(findIdentifier({ a: [{ name: `x ${FAKE_STEAM_ID}` }] })).toBe("body.a[0].name");
    expect(findIdentifier({ name: FAKE_GUID })).toBe("body.name");
    expect(findIdentifier({ name: "123e4567-e89b-12d3-a456-426614174000" })).toBe("body.name");
    expect(findIdentifier({ rows: [{ key: "name:rider a" }] })).toBe("body.rows[0].key");
    expect(findIdentifier({ rows: [{ guid: null }] })).toBe("body.rows[0].guid");
    expect(findIdentifier({ rows: [{ steamId: "x" }] })).toBe("body.rows[0].steamId");
    expect(findIdentifier(body())).toBeNull();
  });

  it("refuses a publish carrying one", () => {
    const withGuid = body();
    (withGuid.rounds[0].results[0] as Record<string, unknown>).name = FAKE_GUID;
    expect(parsePublish(withGuid)).toMatchObject({ error: expect.stringContaining("GUID") });
    const withKey = body({ standings: [{ position: 1, name: "A", points: 1, key: "abc" }] });
    expect(parsePublish(withKey)).toMatchObject({ error: expect.stringContaining("key") });
  });

  it("validates shape and version", () => {
    expect(parsePublish({ ...body(), v: 2 })).toMatchObject({ error: expect.any(String) });
    expect(parsePublish(body({ name: "" }))).toMatchObject({ error: expect.any(String) });
    expect(parsePublish(body({ rounds: [{ round: 1, results: [{ name: "A", status: "crashed" }] }] }))).toMatchObject({ error: expect.stringContaining("status") });
    const ok = parsePublish(body());
    expect("error" in ok).toBe(false);
    if (!("error" in ok)) expect(ok.standings.map((s) => s.name)).toEqual(["Rider A", "Rider B"]);
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
  it("is invisible until published, then readable with CORS", async () => {
    const e = env();
    const token = await reserve(e);
    expect((await call(e, "/v1/series/winter-26")).status).toBe(404);
    expect(await (await call(e, "/v1/series")).json()).toEqual({ series: [] });

    const res = await put(e, "winter-26", token, body());
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ ok: true, url: "https://mxbsecure.com/series/winter-26" });

    const read = await call(e, "/v1/series/winter-26");
    expect(read.status).toBe(200);
    expect(read.headers.get("access-control-allow-origin")).toBe("*");
    const s = (await read.json()) as Record<string, any>;
    expect(s.name).toBe("Winter Series");
    expect(s.nextRound).toEqual({ label: "Round 3", track: "Club MX", startsAt: 1_760_000_000 });
    expect(s.standings[0]).toMatchObject({ position: 1, name: "Rider A", points: 25, mmr: null });
    expect(s.rounds[0].results).toHaveLength(3);
    expect(s.mmrNote).toMatch(/display name/);

    const list = (await (await call(e, "/v1/series")).json()) as { series: any[] };
    expect(list.series[0]).toMatchObject({ slug: "winter-26", rounds: 1, riders: 2, leader: { name: "Rider A", points: 25 } });
  });

  it("rejects bad tokens, other series' tokens and identifier-shaped bodies", async () => {
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

describe("MMR by name", () => {
  async function rate(e: Env, guid: string, name: string, cls: string, rating: number) {
    await e.DB.prepare(
      `INSERT INTO managed_servers (id, label, provider, region, lifecycle, server_url, admin_token, deployment_revision, deployment_method, game_port, created_at, updated_at)
       VALUES ('srv', 'x', 'ovh-vps', 'eu', 'running', 'https://srv.test', 't', 'v', 'systemd', 1, 0, 0) ON CONFLICT DO NOTHING`,
    ).run();
    const race = `srv:e:${guid}:${cls}`;
    await e.DB.prepare(
      "INSERT INTO ingested_races (id, server_id, event_id, race_id, class, session, occurred_at, human_count, rated, created_at) VALUES (?, 'srv', 'e', ?, ?, 'RACE1', 0, 4, 1, 0)",
    ).bind(race, `${guid}:${cls}`, cls).run();
    await e.DB.prepare(
      "INSERT INTO race_results (race_row_id, guid, name, class, position, classified, dnf, dsq, laps_completed, race_laps, is_bot, counted) VALUES (?, ?, ?, ?, 1, 1, 0, 0, 10, 10, 0, 1)",
    ).bind(race, guid, name, cls).run();
    await e.DB.prepare(
      "INSERT INTO rider_ratings (guid, class, rating, rd, volatility, races, updated_at) VALUES (?, ?, ?, 50, 0.06, 7, 0)",
    ).bind(guid, cls, rating).run();
  }

  it("matches by class and name, case-insensitively, and skips shared names and bans", async () => {
    const e = env();
    const token = await reserve(e);
    await rate(e, "G-A", "rider a", "MX1", 1612.4);
    await rate(e, "G-A2", "Rider A", "MX2", 1400);
    await rate(e, "G-B1", "Rider B", "MX1", 1500);
    await rate(e, "G-B2", "Rider B", "MX1", 1550);
    await rate(e, "BANNED", "Rider C", "MX1", 1700);
    expect(await addBan(e, { guid: "BANNED", reason: "cracked content" }, "seed:test")).toMatchObject({ ok: true });
    const standings = ["Rider A", "Rider B", "Rider C"].map((name, i) => ({
      position: i + 1, name, class: "MX1", points: 10 - i, gross_points: 10 - i, wins: 0, rounds_ridden: 1, rounds: [],
    }));
    await put(e, "winter-26", token, body({ standings }));
    const s = (await (await call(e, "/v1/series/winter-26")).json()) as { standings: { name: string; mmr: unknown }[] };
    expect(s.standings[0].mmr).toEqual({ rating: 1612, races: 7, class: "MX1" });
    expect(s.standings[1].mmr).toBeNull(); // two rated riders share the name
    expect(s.standings[2].mmr).toBeNull(); // banned
  });

  it("uses the only class when a standing has none", async () => {
    const e = env();
    const token = await reserve(e);
    await rate(e, "G-A", "Rider A", "MX1", 1612);
    const standings = [{ position: 1, name: "Rider A", class: "", points: 1, gross_points: 1, wins: 0, rounds_ridden: 1, rounds: [] }];
    await put(e, "winter-26", token, body({ standings }));
    const s = (await (await call(e, "/v1/series/winter-26")).json()) as { standings: { mmr: unknown }[] };
    expect(s.standings[0].mmr).toEqual({ rating: 1612, races: 7, class: "MX1" });
  });
});

describe("registration", () => {
  async function published(e: Env, overrides: Record<string, unknown> = {}) {
    const token = await reserve(e);
    await put(e, "winter-26", token, body(overrides));
    return token;
  }
  const entry = { name: "New Rider", number: 41, class: "MX1", team: "Garage", discord: "newrider", website: "" };

  it("stores pending, shows only approved, never Discord", async () => {
    const e = env();
    const token = await published(e);
    const res = await registerReq(e, "winter-26", entry);
    expect(res.status).toBe(201);
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
    expect(await res.json()).toEqual({ ok: true, status: "pending" });

    let s = (await (await call(e, "/v1/series/winter-26")).json()) as { entries: unknown[] };
    expect(s.entries).toEqual([]);

    expect((await call(e, "/v1/series/winter-26/registrations")).status).toBe(401);
    const list = (await (await call(e, "/v1/series/winter-26/registrations", { headers: { Authorization: `Bearer ${token}` } })).json()) as {
      registrations: { id: string; discord: string; status: string }[];
    };
    expect(list.registrations).toHaveLength(1);
    expect(list.registrations[0]).toMatchObject({ discord: "newrider", status: "pending" });

    const id = list.registrations[0].id;
    const decide = await call(e, `/v1/series/winter-26/registrations/${id}`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ status: "approved" }),
    });
    expect(await decide.json()).toEqual({ ok: true, id, status: "approved" });

    s = (await (await call(e, "/v1/series/winter-26")).json()) as { entries: unknown[] };
    expect(s.entries).toEqual([{ name: "New Rider", number: 41, class: "MX1", team: "Garage" }]);
    expect(JSON.stringify(s)).not.toContain("newrider");
  });

  it("refuses duplicates, bad classes, closed series and IDs; swallows the honeypot", async () => {
    const e = env();
    await published(e);
    expect((await registerReq(e, "winter-26", entry)).status).toBe(201);
    expect((await registerReq(e, "winter-26", { ...entry, name: " new rider " })).status).toBe(409);
    expect((await registerReq(e, "winter-26", { ...entry, name: "Other", class: "MX3" })).status).toBe(400);
    expect((await registerReq(e, "winter-26", { ...entry, name: FAKE_STEAM_ID })).status).toBe(400);
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
