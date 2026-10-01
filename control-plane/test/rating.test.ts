import { describe, expect, it } from "vitest";
import { addBan } from "../src/bans";
import {
  adminRiderLookup,
  ingestResults as rawIngest,
  issueRatingToken,
  leaderboard,
  myRatings,
  opaqueRiderId,
} from "../src/rating";
import { d1 } from "./d1sqlite";

/**
 * Only app-verified GUIDs (bound to an account) are rated, so the helper below binds every GUID
 * in a push to an account before ingesting it — except those prefixed UNVERIFIED and nulls,
 * which is exactly what the unverified tests need. The raw ingest is used where that matters.
 */
async function verify(e: Env, ...guids: string[]): Promise<void> {
  for (const g of guids) {
    await e.DB.prepare(
      "INSERT OR IGNORE INTO accounts (id, rider_name, token_hash, guid, created_at) VALUES (?, ?, ?, ?, ?)",
    )
      .bind(`acct_${g}`, `rider_${g}`, `hash_${g}`, g, Date.now())
      .run();
  }
}

async function ingestResults(request: Request, e: Env) {
  const body = (await request.clone().json()) as { riders?: { guid?: unknown }[] };
  const guids = (body.riders ?? []).map((r) => r.guid).filter((g): g is string => typeof g === "string" && !g.startsWith("UNVERIFIED"));
  await verify(e, ...guids);
  return rawIngest(request, e);
}

function env(): Env {
  return { DB: d1() } as unknown as Env;
}

async function addManagedServer(e: Env, id = "srv_1"): Promise<void> {
  const now = Date.now();
  await e.DB.prepare(
    `INSERT INTO managed_servers
       (id, label, provider, region, lifecycle, game_endpoint, server_url, admin_token,
        deployment_revision, deployment_method, game_port, created_at, updated_at)
     VALUES (?, 'race host', 'ovh-vps', 'us-west', 'running', NULL, ?, 'admin-token-thats-long-enough', 'v1', 'systemd', 54210, ?, ?)`,
  )
    .bind(id, `https://${id}.example.test`, now, now)
    .run();
}

function rider(
  guid: string | null,
  overrides: Partial<{
    name: string;
    position: number | null;
    classified: boolean;
    dnf: boolean;
    dsq: boolean;
    lapsCompleted: number;
    raceLaps: number;
    isBot: boolean;
  }> = {},
) {
  return {
    guid,
    name: overrides.name ?? guid ?? "nameless",
    position: overrides.position ?? 1,
    classified: overrides.classified ?? true,
    dnf: overrides.dnf ?? false,
    dsq: overrides.dsq ?? false,
    lapsCompleted: overrides.lapsCompleted ?? 10,
    raceLaps: overrides.raceLaps ?? 10,
    isBot: overrides.isBot ?? false,
  };
}

function push(token: string, riders: ReturnType<typeof rider>[], overrides: Record<string, unknown> = {}) {
  return new Request("https://cp.test/v1/rating/ingest", {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify({
      v: 1,
      serverId: "srv_1",
      eventId: "evt_1",
      raceId: "race_1",
      track: "loam-ridge",
      class: "MX1",
      session: "race1",
      timestamp: Date.now(),
      riders,
      ...overrides,
    }),
  });
}

async function issueToken(e: Env, serverId = "srv_1"): Promise<string> {
  const result = await issueRatingToken(e, serverId);
  return (result.body as { token: string }).token;
}

describe("rating token issuance", () => {
  it("issues a token only for a known managed server, shown once", async () => {
    const e = env();
    expect((await issueRatingToken(e, "nope")).status).toBe(404);

    await addManagedServer(e);
    const issued = await issueRatingToken(e, "srv_1");
    expect(issued.status).toBe(200);
    const token = (issued.body as { token: string }).token;
    expect(token.length).toBeGreaterThan(20);

    const row = await e.DB.prepare("SELECT rating_token_hash FROM managed_servers WHERE id = 'srv_1'").first<{
      rating_token_hash: string | null;
    }>();
    expect(row?.rating_token_hash).toBeTruthy();
    expect(row?.rating_token_hash).not.toBe(token);
  });

  it("rotating a token invalidates the previous one", async () => {
    const e = env();
    await addManagedServer(e);
    const first = await issueToken(e);
    const second = await issueToken(e);
    expect(first).not.toBe(second);

    const withOld = await ingestResults(push(first, [rider("GID1"), rider("GID2"), rider("GID3"), rider("GID4")]), e);
    expect(withOld.status).toBe(401);
    const withNew = await ingestResults(push(second, [rider("GID1"), rider("GID2"), rider("GID3"), rider("GID4")]), e);
    expect(withNew.status).toBe(201);
  });
});

describe("ingestResults", () => {
  it("rejects an unknown token and stores nothing", async () => {
    const e = env();
    await addManagedServer(e);
    const result = await ingestResults(push("not-a-real-token", [rider("GID1")]), e);
    expect(result.status).toBe(401);
    const count = await e.DB.prepare("SELECT COUNT(*) AS n FROM ingested_races").first<{ n: number }>();
    expect(count?.n).toBe(0);
  });

  it("rejects an unversioned or malformed payload", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const badVersion = new Request("https://cp.test/v1/rating/ingest", {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      body: JSON.stringify({ v: 2, serverId: "srv_1" }),
    });
    expect((await ingestResults(badVersion, e)).status).toBe(400);
  });

  it("stores a race raw but does not rate it under 4 human finishers", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const result = await ingestResults(push(token, [rider("GID1"), rider("GID2"), rider("GID3")]), e);
    expect(result.status).toBe(201);
    expect((result.body as { rated: boolean }).rated).toBe(false);

    const race = await e.DB.prepare("SELECT rated, human_count FROM ingested_races WHERE id = 'srv_1:evt_1:race_1'").first<{
      rated: number;
      human_count: number;
    }>();
    expect(race?.rated).toBe(0);
    expect(race?.human_count).toBe(3);

    const ratings = await e.DB.prepare("SELECT COUNT(*) AS n FROM rider_ratings").first<{ n: number }>();
    expect(ratings?.n).toBe(0);
  });

  it("excludes bots from the human count, even with the bot present in the payload", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const result = await ingestResults(
      push(token, [
        rider("GID1", { position: 1 }),
        rider("GID2", { position: 2 }),
        rider("GID3", { position: 3 }),
        rider("BOTGUID", { isBot: true, position: 4 }),
      ]),
      e,
    );
    expect((result.body as { rated: boolean }).rated).toBe(false);
  });

  it("rates a race with 4+ humans and excludes a rider under the 50% laps floor", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const result = await ingestResults(
      push(token, [
        rider("WINNER", { position: 1 }),
        rider("SECOND", { position: 2 }),
        rider("THIRD", { position: 3 }),
        rider("CRASHED", { dnf: true, classified: false, lapsCompleted: 1, raceLaps: 10 }),
      ]),
      e,
    );
    expect(result.status).toBe(201);
    expect((result.body as { rated: boolean }).rated).toBe(true);

    const rated = await e.DB.prepare("SELECT guid FROM rider_ratings ORDER BY guid").all<{ guid: string }>();
    expect(rated.results.map((r) => r.guid)).toEqual(["SECOND", "THIRD", "WINNER"]);

    const winner = await e.DB.prepare("SELECT rating FROM rider_ratings WHERE guid = 'WINNER'").first<{ rating: number }>();
    const third = await e.DB.prepare("SELECT rating FROM rider_ratings WHERE guid = 'THIRD'").first<{ rating: number }>();
    expect(winner!.rating).toBeGreaterThan(third!.rating);

    const crashedRow = await e.DB.prepare("SELECT counted FROM race_results WHERE guid = 'CRASHED'").first<{ counted: number }>();
    expect(crashedRow?.counted).toBe(0);
  });

  it("ranks a DNF with meaningful mileage below classified finishers but still rates them", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await ingestResults(
      push(token, [
        rider("WINNER", { position: 1 }),
        rider("SECOND", { position: 2 }),
        rider("THIRD", { position: 3 }),
        rider("RETIRED", { dnf: true, classified: false, lapsCompleted: 7, raceLaps: 10 }),
      ]),
      e,
    );
    const retired = await e.DB.prepare("SELECT counted, rank_order FROM race_results WHERE guid = 'RETIRED'").first<{
      counted: number;
      rank_order: number;
    }>();
    expect(retired?.counted).toBe(1);
    expect(retired?.rank_order).toBe(3); // last of the four rated riders (0-indexed)
  });

  it("is idempotent by server+event+race id: a re-push is a no-op", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const riders = [rider("GID1"), rider("GID2", { position: 2 }), rider("GID3", { position: 3 }), rider("GID4", { position: 4 })];
    const first = await ingestResults(push(token, riders), e);
    expect(first.status).toBe(201);
    const second = await ingestResults(push(token, riders), e);
    expect(second.status).toBe(200);
    expect((second.body as { alreadyIngested: boolean }).alreadyIngested).toBe(true);

    const races = await e.DB.prepare("SELECT COUNT(*) AS n FROM ingested_races").first<{ n: number }>();
    expect(races?.n).toBe(1);
    const history = await e.DB.prepare("SELECT COUNT(*) AS n FROM rating_history").first<{ n: number }>();
    expect(history?.n).toBe(4);
  });

  it("rejects a serverId in the body that doesn't match the token's own server", async () => {
    const e = env();
    await addManagedServer(e, "srv_1");
    await addManagedServer(e, "srv_2");
    const token = await issueToken(e, "srv_2");
    const result = await ingestResults(push(token, [rider("GID1"), rider("GID2"), rider("GID3"), rider("GID4")]), e);
    expect(result.status).toBe(400);
  });
});

describe("app-verified-only rating", () => {
  it("stores unverified and null-guid riders raw but never rates them, and does not reject the push", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await verify(e, "FF0000000000000001", "FF0000000000000002", "FF0000000000000003");
    const result = await rawIngest(
      push(token, [
        rider("FF0000000000000001", { position: 1 }),
        rider("FF0000000000000002", { position: 2 }),
        rider("UNVERIFIED0000000001", { position: 3 }),
        rider(null, { position: 4 }),
        rider("FF0000000000000003", { position: 5 }),
      ]),
      e,
    );
    expect(result.status).toBe(201);
    expect((result.body as { rated: boolean; ratedRiders: number }).ratedRiders).toBe(3);

    const raw = await e.DB.prepare("SELECT COUNT(*) AS n FROM race_results").first<{ n: number }>();
    expect(raw?.n).toBe(5);
    const nullRow = await e.DB.prepare("SELECT counted FROM race_results WHERE guid IS NULL").first<{ counted: number }>();
    expect(nullRow?.counted).toBe(0);
    const unverified = await e.DB.prepare("SELECT counted FROM race_results WHERE guid = 'UNVERIFIED0000000001'").first<{ counted: number }>();
    expect(unverified?.counted).toBe(0);

    const rated = await e.DB.prepare("SELECT guid FROM rider_ratings ORDER BY guid").all<{ guid: string }>();
    expect(rated.results.map((r) => r.guid)).toEqual(["FF0000000000000001", "FF0000000000000002", "FF0000000000000003"]);
  });

  it("accepts a push where a rider has no guid field or a malformed one", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const req = push(token, [rider("AAAA1", { position: 1 }), rider("AAAA2", { position: 2 }), rider("AAAA3", { position: 3 }), rider("AAAA4", { position: 4 })]);
    const body = (await req.clone().json()) as { riders: Record<string, unknown>[] };
    delete body.riders[0].guid;
    body.riders[1].guid = "bad guid with spaces!";
    body.riders[2].guid = 12345;
    const result = await rawIngest(
      new Request(req.url, { method: "POST", headers: req.headers, body: JSON.stringify(body) }),
      e,
    );
    expect(result.status).toBe(201);
    const nulls = await e.DB.prepare("SELECT COUNT(*) AS n FROM race_results WHERE guid IS NULL").first<{ n: number }>();
    expect(nulls?.n).toBe(3);
  });

  it("starts rating a rider once they claim their guid, from the next ingest of that class", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await verify(e, "FF0000000000000001", "FF0000000000000002", "FF0000000000000003");
    const first = [
      rider("FF0000000000000001", { position: 1 }),
      rider("FF0000000000000002", { position: 2 }),
      rider("FF0000000000000003", { position: 3 }),
      rider("LATECLAIM000001", { position: 4 }),
    ];
    await rawIngest(push(token, first, { raceId: "r1", timestamp: 1000 }), e);
    expect(await e.DB.prepare("SELECT 1 FROM rider_ratings WHERE guid = 'LATECLAIM000001'").first()).toBeNull();
    await verify(e, "LATECLAIM000001");
    await rawIngest(push(token, first, { raceId: "r2", timestamp: 2000 }), e);
    const row = await e.DB.prepare("SELECT races FROM rider_ratings WHERE guid = 'LATECLAIM000001'").first<{ races: number }>();
    expect(row?.races).toBe(2); // the earlier race is picked up too: replay is from the stored rows
  });
});

describe("ingest rate limit", () => {
  it("returns 429 once the per-server limiter says no, keyed by server, and stores nothing", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const keys: string[] = [];
    let allowed = 1;
    (e as unknown as { INGEST_LIMITER: unknown }).INGEST_LIMITER = {
      limit: async ({ key }: { key: string }) => {
        keys.push(key);
        return { success: allowed-- > 0 };
      },
    };
    const riders = [rider("G1", { position: 1 }), rider("G2", { position: 2 }), rider("G3", { position: 3 }), rider("G4", { position: 4 })];
    expect((await ingestResults(push(token, riders, { raceId: "a" }), e)).status).toBe(201);
    const limited = await ingestResults(push(token, riders, { raceId: "b" }), e);
    expect(limited.status).toBe(429);
    expect(keys).toEqual(["srv_1", "srv_1"]);
    const races = await e.DB.prepare("SELECT COUNT(*) AS n FROM ingested_races").first<{ n: number }>();
    expect(races?.n).toBe(1);
  });

  it("does not spend the limiter on unauthenticated requests", async () => {
    const e = env();
    await addManagedServer(e);
    let calls = 0;
    (e as unknown as { INGEST_LIMITER: unknown }).INGEST_LIMITER = {
      limit: async () => {
        calls++;
        return { success: true };
      },
    };
    expect((await rawIngest(push("not-a-token", [rider("G1")]), e)).status).toBe(401);
    expect(calls).toBe(0);
  });
});

describe("out-of-order arrival", () => {
  const ids = ["FF0000000000000001", "FF0000000000000002", "FF0000000000000003", "FF0000000000000004"];
  const race = (order: string[]) => order.map((g, i) => rider(g, { position: i + 1 }));
  const races = [
    { raceId: "r1", timestamp: 1_000_000, order: ids },
    { raceId: "r2", timestamp: 2_000_000, order: [ids[1], ids[0], ids[3], ids[2]] },
    { raceId: "r3", timestamp: 3_000_000, order: [ids[2], ids[3], ids[1], ids[0]] },
  ];

  async function run(arrival: number[]) {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    for (const i of arrival) {
      const r = races[i];
      const res = await ingestResults(push(token, race(r.order), { raceId: r.raceId, timestamp: r.timestamp }), e);
      expect(res.status).toBe(201);
    }
    const ratings = await e.DB.prepare("SELECT guid, rating, rd, volatility, races FROM rider_ratings ORDER BY guid").all();
    const history = await e.DB.prepare(
      "SELECT guid, race_row_id, rating_before, rating_after, created_at FROM rating_history ORDER BY created_at, guid",
    ).all();
    return { ratings: ratings.results, history: history.results };
  }

  it("yields identical ratings and history whatever order the races arrive in", async () => {
    const inOrder = await run([0, 1, 2]);
    expect(inOrder.history).toHaveLength(12);
    expect(await run([2, 1, 0])).toEqual(inOrder);
    expect(await run([1, 2, 0])).toEqual(inOrder);
    expect(await run([2, 0, 1])).toEqual(inOrder);
  });

  it("a late race slots into its chronological place and stays idempotent on re-push", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    const send = (i: number) => ingestResults(push(token, race(races[i].order), { raceId: races[i].raceId, timestamp: races[i].timestamp }), e);
    await send(2);
    await send(0);
    const before = await e.DB.prepare("SELECT guid, rating FROM rider_ratings ORDER BY guid").all();
    const dup = await send(0);
    expect(dup.status).toBe(200);
    expect((dup.body as { alreadyIngested: boolean }).alreadyIngested).toBe(true);
    expect((await e.DB.prepare("SELECT guid, rating FROM rider_ratings ORDER BY guid").all()).results).toEqual(before.results);
    await send(1); // arrives last, happened in the middle
    const firstRow = await e.DB.prepare(
      "SELECT race_row_id FROM rating_history WHERE guid = ? ORDER BY created_at LIMIT 1",
    )
      .bind(ids[0])
      .first<{ race_row_id: string }>();
    expect(firstRow?.race_row_id).toBe("srv_1:evt_1:r1");
    const counts = await e.DB.prepare("SELECT COUNT(*) AS n FROM ingested_races").first<{ n: number }>();
    expect(counts?.n).toBe(3);
  });
});

describe("leaderboard", () => {
  it("never shows a banned rider, RD/volatility or the raw guid", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await ingestResults(
      push(token, [
        rider("CLEAN1", { position: 1 }),
        rider("CLEAN2", { position: 2 }),
        rider("BANNED", { position: 3 }),
        rider("CLEAN3", { position: 4 }),
      ]),
      e,
    );
    await addBan(e, { guid: "BANNED", reason: "cracked content" }, "seed:test");

    const result = await leaderboard(e, "MX1", 50, false);
    expect(result.status).toBe(200);
    const riders = (result.body as { riders: { id: string; name: string }[] }).riders;
    expect(riders.map((r) => r.name)).not.toContain("BANNED");
    expect(riders.every((r) => !("guid" in r) && !("rd" in r) && !("volatility" in r))).toBe(true);
    expect(riders[0].id).toBe(await opaqueRiderId("CLEAN1"));
  });

  it("admin leaderboard includes banned riders", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await ingestResults(
      push(token, [
        rider("CLEAN1", { position: 1 }),
        rider("CLEAN2", { position: 2 }),
        rider("BANNED", { position: 3 }),
        rider("CLEAN3", { position: 4 }),
      ]),
      e,
    );
    await addBan(e, { guid: "BANNED", reason: "cracked content" }, "seed:test");

    const result = await leaderboard(e, "MX1", 50, true);
    const riders = (result.body as { riders: { name: string }[] }).riders;
    expect(riders.map((r) => r.name)).toContain("BANNED");
  });
});

describe("myRatings / adminRiderLookup", () => {
  it("reports unlinked when the account has no guid", async () => {
    const e = env();
    const result = await myRatings(e, null);
    expect(result.body).toEqual({ linked: false, classes: [] });
  });

  it("returns the signed-in rider's own ratings across classes, with RD/volatility", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await ingestResults(
      push(token, [
        rider("MEGUID", { position: 1 }),
        rider("GID2", { position: 2 }),
        rider("GID3", { position: 3 }),
        rider("GID4", { position: 4 }),
      ]),
      e,
    );
    const mine = await myRatings(e, "MEGUID");
    const body = mine.body as { linked: boolean; classes: { class: string; rd: number }[] };
    expect(body.linked).toBe(true);
    expect(body.classes[0].class).toBe("MX1");
    expect(body.classes[0].rd).toBeGreaterThan(0);
  });

  it("admin lookup surfaces ban status alongside the raw guid", async () => {
    const e = env();
    await addManagedServer(e);
    const token = await issueToken(e);
    await ingestResults(
      push(token, [
        rider("FLAGGED", { position: 1 }),
        rider("GID2", { position: 2 }),
        rider("GID3", { position: 3 }),
        rider("GID4", { position: 4 }),
      ]),
      e,
    );
    await addBan(e, { guid: "FLAGGED", reason: "sandbagging" }, "seed:test");
    const result = await adminRiderLookup(e, "FLAGGED");
    expect((result.body as { banned: boolean }).banned).toBe(true);
  });
});
