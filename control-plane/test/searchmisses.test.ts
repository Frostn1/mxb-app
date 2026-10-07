import { describe, expect, it } from "vitest";

import {
  collectMisses,
  MAX_MISSES_PER_DAY,
  MAX_QUERY_CHARS,
  missStats,
  missWindow,
  normaliseQuery,
  parseMiss,
  pruneSearchMisses,
  reportMiss,
  RETENTION_DAYS,
} from "../src/searchmisses";
import { dayKey } from "../src/usage";
import { d1 } from "./d1sqlite";

function post(payload: unknown, headers: Record<string, string> = {}): Request {
  return new Request("https://cp.test/v1/search-misses", {
    method: "POST",
    headers: { "content-type": "application/json", ...headers },
    body: typeof payload === "string" ? payload : JSON.stringify(payload),
  });
}

function env(db: Env["DB"], over: Record<string, unknown> = {}): Env {
  return { DB: db, ...over } as unknown as Env;
}

describe("a search miss", () => {
  it("is normalised and carries nothing else", () => {
    expect(parseMiss(JSON.stringify({ query: "  Honda  CR250 ", game: "mxb", installId: "x" }))).toEqual({
      query: "honda cr250",
      game: "mxb",
    });
  });

  it("caps the query", () => {
    expect(normaliseQuery("x".repeat(500))?.length).toBe(MAX_QUERY_CHARS);
  });

  it("refuses short, empty and non-string queries", () => {
    expect(parseMiss(JSON.stringify({ query: "a" }))).toBe("query is not usable");
    expect(parseMiss(JSON.stringify({ query: 5 }))).toBe("query is not usable");
    expect(parseMiss("nope")).toBe("expected JSON");
    expect(parseMiss("[]")).toBe("expected an object");
  });

  it("drops queries that carry an identifier", () => {
    for (const q of [
      "me@example.com",
      "https://x.test/a",
      "C:\\Users\\Frost\\mods",
      "76561198000000000",
      "123e4567-e89b-12d3-a456-426614174000",
    ]) {
      expect(normaliseQuery(q)).toBeNull();
    }
  });

  it("needs a slug for the game", () => {
    expect(parseMiss(JSON.stringify({ query: "foo", game: "No Way!" }))).toBe("game must be a slug");
    expect(parseMiss(JSON.stringify({ query: "foo" }))).toEqual({ query: "foo", game: "" });
  });
});

describe("POST /v1/search-misses", () => {
  it("counts repeats per day and stores only text, game and a day", async () => {
    const db = d1();
    expect((await reportMiss(post({ query: "Foo", game: "mxb" }), env(db))).status).toBe(202);
    expect((await reportMiss(post({ query: "foo", game: "mxb" }), env(db))).status).toBe(202);

    const info = await db.prepare("PRAGMA table_info(search_misses)").all<{ name: string }>();
    expect(info.results?.map((c) => c.name)).toEqual(["day", "query", "game", "misses"]);
    const rows = await db.prepare("SELECT query, game, misses FROM search_misses").all();
    expect(rows.results).toEqual([{ query: "foo", game: "mxb", misses: 2 }]);
  });

  it("insists on JSON", async () => {
    const req = new Request("https://cp.test/v1/search-misses", {
      method: "POST",
      headers: { "content-type": "text/plain" },
      body: JSON.stringify({ query: "foo" }),
    });
    expect((await reportMiss(req, env(d1()))).status).toBe(415);
  });

  it("refuses a body far too big", async () => {
    const req = post({ query: "foo" }, { "content-length": String(64 * 1024) });
    expect((await reportMiss(req, env(d1()))).status).toBe(413);
  });

  it("refuses an unusable query", async () => {
    expect((await reportMiss(post({ query: "x" }), env(d1()))).status).toBe(400);
  });

  it("is refused unsigned once the deployment requires a signature", async () => {
    const strict = env(d1(), { MXB_USAGE_REQUIRE_SIGNATURE: "1", USAGE_SIGNING_KEY: "k" });
    expect((await reportMiss(post({ query: "foo" }), strict)).status).toBe(401);
  });

  it("rate limits an address per day", async () => {
    const db = d1();
    const headers = { "CF-Connecting-IP": "203.0.113.9" };
    for (let i = 0; i < MAX_MISSES_PER_DAY; i++) {
      expect((await reportMiss(post({ query: `query ${i}` }, headers), env(db))).status).toBe(202);
    }
    expect((await reportMiss(post({ query: "one more" }, headers), env(db))).status).toBe(429);
  });
});

describe("the read and the sweep", () => {
  async function seed(db: Env["DB"], rows: [string, string, string, number][]) {
    for (const [day, query, game, misses] of rows) {
      await db
        .prepare("INSERT INTO search_misses (day, query, game, misses) VALUES (?, ?, ?, ?)")
        .bind(day, query, game, misses)
        .run();
    }
  }

  it("ranks missed queries over the window", async () => {
    const db = d1();
    const now = Date.now();
    await seed(db, [
      [dayKey(now), "alpha", "mxb", 2],
      [dayKey(now, 1), "alpha", "mxb", 3],
      [dayKey(now), "beta", "mxb", 4],
      [dayKey(now, 40), "ancient", "mxb", 99],
    ]);
    const stats = await collectMisses(env(db), 30, now);
    expect(stats.top).toEqual([
      { query: "alpha", game: "mxb", misses: 5, days: 2, lastSeen: dayKey(now) },
      { query: "beta", game: "mxb", misses: 4, days: 1, lastSeen: dayKey(now) },
    ]);
  });

  it("clamps the window", () => {
    expect(missWindow(new URL("https://cp.test/x?days=9999"))).toBe(90);
    expect(missWindow(new URL("https://cp.test/x?days=nope"))).toBe(30);
  });

  it("is gated like the survey stats", async () => {
    const db = d1();
    const url = new URL("https://cp.test/v1/search-misses/stats");
    const bare = new Request(url);
    expect((await missStats(bare, url, env(db))).status).toBe(503);
    const keyed = env(db, { ADMIN_KEY: "secret" });
    expect((await missStats(bare, url, keyed)).status).toBe(401);
    const authed = new Request(url, { headers: { Authorization: "Bearer secret" } });
    expect((await missStats(authed, url, keyed)).status).toBe(200);
  });

  it("prunes rows past retention", async () => {
    const db = d1();
    const now = Date.now();
    await seed(db, [
      [dayKey(now, RETENTION_DAYS + 1), "old", "mxb", 1],
      [dayKey(now, 1), "new", "mxb", 1],
    ]);
    await pruneSearchMisses(env(db));
    const rows = await db.prepare("SELECT query FROM search_misses").all<{ query: string }>();
    expect(rows.results).toEqual([{ query: "new" }]);
  });
});
