import { describe, expect, it } from "vitest";
import { addBan } from "../src/bans";
import {
  adminRiderLookup,
  ingestResults,
  issueRatingToken,
  leaderboard,
  myRatings,
  opaqueRiderId,
} from "../src/rating";
import { d1 } from "./d1sqlite";

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
  guid: string,
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
    name: overrides.name ?? guid,
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
