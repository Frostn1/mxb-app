import { describe, expect, it } from "vitest";

import {
  adminAllowed,
  collectStats,
  MAX_REPORT_BYTES,
  MAX_REPORTS_PER_DAY,
  parseReport,
  reportUsage,
  usageStats,
  windowDays,
} from "../src/usage";
import { MAX_EVENTS_PER_REPORT } from "../src/validate";

const INSTALL = "6f1f2b6c-0f6d-4a5e-9f3a-2b7c4d5e6f70";

/** A report the endpoint would accept, so each test can spoil one thing about it. */
function body(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    installId: INSTALL,
    version: "0.12.3",
    os: "windows",
    game: "mxb",
    sessions: 1,
    minutes: 12,
    events: [{ name: "view.browse", count: 3 }],
    ...over,
  };
}

function post(payload: unknown): Request {
  return new Request("https://cp.test/v1/usage", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: typeof payload === "string" ? payload : JSON.stringify(payload),
  });
}

/** A D1 stand-in that records every statement and its bindings. */
function stubDb(claims: number | null = null) {
  const statements: { sql: string; args: unknown[] }[] = [];
  const db = {
    statements,
    prepare(sql: string) {
      return {
        bind(...args: unknown[]) {
          const stmt = { sql, args };
          return {
            stmt,
            async first() {
              if (sql.includes("FROM device_claims")) return claims === null ? null : { claims };
              return null;
            },
            async all() {
              return { results: [] };
            },
            async run() {
              statements.push(stmt);
            },
          };
        },
      };
    },
    async batch(prepared: { stmt: { sql: string; args: unknown[] } }[]) {
      for (const p of prepared) statements.push(p.stmt);
      return [];
    },
  };
  return db;
}

describe("what a report has to be", () => {
  it("takes a well-formed one", () => {
    expect(parseReport(JSON.stringify(body()))).toMatchObject({
      installId: INSTALL,
      events: [{ name: "view.browse", count: 3 }],
    });
  });

  it("refuses anything that isn't a UUID as the install id", () => {
    // The point of the shape: a client sending a rider name here is rejected, not stored.
    expect(parseReport(JSON.stringify(body({ installId: "Ryan" })))).toBe("installId must be a UUID");
  });

  it("refuses a version that isn't semver", () => {
    expect(parseReport(JSON.stringify(body({ version: "nightly" })))).toContain("semver");
  });

  it("refuses an OS it doesn't build for", () => {
    expect(parseReport(JSON.stringify(body({ os: "haiku" })))).toContain("windows");
  });

  it("refuses an event name carrying anything but a name", () => {
    const bad = body({ events: [{ name: "mod.install:C:/Users/ryan/mods", count: 1 }] });
    expect(parseReport(JSON.stringify(bad))).toContain("not an event name");
  });

  it("refuses a count past the cap", () => {
    expect(parseReport(JSON.stringify(body({ events: [{ name: "app.start", count: 1e9 }] })))).toBe(
      "event count out of range",
    );
  });

  it("refuses more events than one session could hold", () => {
    const events = Array.from({ length: MAX_EVENTS_PER_REPORT + 1 }, (_, i) => ({
      name: `view.tab${i}`,
      count: 1,
    }));
    expect(parseReport(JSON.stringify(body({ events })))).toBe("too many events in one report");
  });

  it("adds up a name that arrives twice rather than losing one", () => {
    const twice = body({
      events: [
        { name: "app.start", count: 1 },
        { name: "app.start", count: 2 },
      ],
    });
    expect(parseReport(JSON.stringify(twice))).toMatchObject({
      events: [{ name: "app.start", count: 3 }],
    });
  });

  it("refuses a body that isn't JSON", () => {
    expect(parseReport("not json")).toBe("expected a JSON body");
  });
});

describe("the endpoint", () => {
  it("writes the day row, the rate counter and one row per event", async () => {
    const db = stubDb();
    const res = await reportUsage(post(body()), { DB: db } as unknown as Env);

    expect(res.status).toBe(202);
    const sql = db.statements.map((s) => s.sql);
    expect(sql.some((s) => s.includes("INTO usage_daily"))).toBe(true);
    expect(sql.some((s) => s.includes("INTO device_claims"))).toBe(true);
    expect(sql.filter((s) => s.includes("INTO usage_events"))).toHaveLength(1);
  });

  it("adds to the day's counters instead of replacing them", async () => {
    const db = stubDb();
    await reportUsage(post(body()), { DB: db } as unknown as Env);

    const daily = db.statements.find((s) => s.sql.includes("INTO usage_daily"))!;
    expect(daily.sql).toContain("sessions = sessions + excluded.sessions");
    expect(daily.sql).toContain("minutes = minutes + excluded.minutes");
    const event = db.statements.find((s) => s.sql.includes("INTO usage_events"))!;
    expect(event.sql).toContain("count = count + excluded.count");
  });

  it("keys the row on the install and the UTC day, and stores nothing else about the caller", async () => {
    const db = stubDb();
    await reportUsage(post(body()), { DB: db } as unknown as Env);

    const daily = db.statements.find((s) => s.sql.includes("INTO usage_daily"))!;
    expect(daily.args[0]).toBe(INSTALL);
    expect(daily.args[1]).toBe(new Date().toISOString().slice(0, 10));
    // The address never reaches a column: only its daily digest, in the rate counter.
    const claims = db.statements.find((s) => s.sql.includes("INTO device_claims"))!;
    expect(String(claims.args[0])).toMatch(/^[0-9a-f]{64}$/);
  });

  it("says no to a bad report before touching the database", async () => {
    const db = stubDb();
    const res = await reportUsage(post(body({ installId: "nope" })), { DB: db } as unknown as Env);

    expect(res.status).toBe(400);
    expect(db.statements).toHaveLength(0);
  });

  it("refuses an oversized body on its declared length alone", async () => {
    const db = stubDb();
    const req = new Request("https://cp.test/v1/usage", {
      method: "POST",
      headers: { "content-length": String(MAX_REPORT_BYTES + 1) },
      body: JSON.stringify(body()),
    });

    expect((await reportUsage(req, { DB: db } as unknown as Env)).status).toBe(413);
    expect(db.statements).toHaveLength(0);
  });

  it("counts a signup and a usage report separately", async () => {
    const db = stubDb();
    await reportUsage(post(body()), { DB: db } as unknown as Env);

    const claims = db.statements.find((s) => s.sql.includes("INTO device_claims"))!;
    expect(claims.sql).toContain("'usage'");
  });
});

describe("what a rate-limited caller is told", () => {
  it("answers 429 once an address is past its cap, so the client stops asking", async () => {
    // The client treats 429 as "stop reporting this run" — see `usage.rs`. It must therefore
    // actually see one, rather than the 202-and-ignore this used to answer with: a few
    // hundred installs each knocking every half hour is exactly how a daily ceiling is
    // reached, and a counter is not worth being part of that.
    const db = stubDb(MAX_REPORTS_PER_DAY);
    const res = await reportUsage(post(body()), { DB: db } as unknown as Env);

    expect(res.status).toBe(429);
    expect(db.statements).toHaveLength(0);
  });
});

describe("who may read the numbers", () => {
  const url = (query = "") => new URL(`https://cp.test/v1/usage/stats${query}`);
  const plain = new Request("https://cp.test/v1/usage/stats");

  it("has no admin surface at all on a deployment with no key", () => {
    expect(adminAllowed(plain, url(), {} as Env)).toBe("unset");
  });

  it("turns away a request with no key", () => {
    expect(adminAllowed(plain, url(), { ADMIN_KEY: "s3cret" } as Env)).toBe("denied");
  });

  it("turns away the wrong key", () => {
    expect(adminAllowed(plain, url("?key=guess"), { ADMIN_KEY: "s3cret" } as Env)).toBe("denied");
  });

  it("takes the key from the query, because a browser cannot send a header", () => {
    expect(adminAllowed(plain, url("?key=s3cret"), { ADMIN_KEY: "s3cret" } as Env)).toBe("ok");
  });

  it("takes a bearer token too, for anything scripting it", () => {
    const req = new Request("https://cp.test/v1/usage/stats", {
      headers: { Authorization: "Bearer s3cret" },
    });
    expect(adminAllowed(req, url(), { ADMIN_KEY: "s3cret" } as Env)).toBe("ok");
  });

  it("answers 503 rather than 401 when nobody configured a key", async () => {
    const res = await usageStats(plain, url(), { DB: stubDb() } as unknown as Env);
    expect(res.status).toBe(503);
  });

  it("answers 401 with a key configured and none presented", async () => {
    const env = { DB: stubDb(), ADMIN_KEY: "s3cret" } as unknown as Env;
    expect((await usageStats(plain, url(), env)).status).toBe(401);
  });
});

describe("the window", () => {
  it("defaults to a month", () => {
    expect(windowDays(new URL("https://cp.test/admin/usage"))).toBe(30);
  });

  it("clamps something absurd rather than scanning the whole history", () => {
    expect(windowDays(new URL("https://cp.test/admin/usage?days=99999"))).toBe(365);
    expect(windowDays(new URL("https://cp.test/admin/usage?days=-4"))).toBe(1);
    expect(windowDays(new URL("https://cp.test/admin/usage?days=nonsense"))).toBe(30);
  });
});

describe("reading it back", () => {
  it("survives an empty database, which is what the first day looks like", async () => {
    const stats = await collectStats({ DB: stubDb() } as unknown as Env, 30);

    expect(stats.active).toEqual({ day: 0, week: 0, month: 0 });
    expect(stats.events).toEqual([]);
    expect(stats.currentVersions).toEqual([]);
    // Everything the app can report is listed as untouched, rather than the page being blank.
    expect(stats.unused).toContain("view.browse");
  });

  it("counts an install once per version it ran, and once overall", async () => {
    // Two installs on 0.13.5, one of which was on 0.12.6 earlier in the window. "Seen"
    // counts it twice on purpose; "now" is what says how many are actually on each build.
    const db = answering({
      "GROUP BY version": [
        { label: "0.13.5", installs: 2 },
        { label: "0.12.6", installs: 1 },
      ],
      "PARTITION BY install_id": [
        { label: "0.13.5", installs: 2 },
      ],
    });
    const stats = await collectStats({ DB: db } as unknown as Env, 30);

    expect(stats.versions).toHaveLength(2);
    expect(stats.currentVersions).toEqual([{ label: "0.13.5", installs: 2 }]);
  });

  it("asks for the latest day per install, not every day it reported", async () => {
    const db = answering({});
    await collectStats({ DB: db } as unknown as Env, 30);
    const sql = db.asked.find((q) => q.includes("PARTITION BY install_id"));

    expect(sql).toContain("ORDER BY day DESC");
    expect(sql).toContain("WHERE rn = 1");
  });
});

/** A D1 stand-in for reads: every query gets the rows whose key its SQL contains. */
function answering(rows: Record<string, unknown[]>) {
  const asked: string[] = [];
  return {
    asked,
    prepare(sql: string) {
      asked.push(sql);
      return {
        bind() {
          return {
            async all() {
              const key = Object.keys(rows).find((k) => sql.includes(k));
              return { results: key ? rows[key] : [] };
            },
          };
        },
      };
    },
  };
}
