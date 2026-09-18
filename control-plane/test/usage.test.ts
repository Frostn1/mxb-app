import { describe, expect, it } from "vitest";

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import {
  adminAllowed,
  collectStats,
  KNOWN_EVENTS,
  knownFor,
  MAX_DAY_MINUTES,
  MAX_DAY_SESSIONS,
  MAX_REPORT_BYTES,
  MAX_REPORTS_PER_DAY,
  MAX_SIGNATURE_SKEW_SECONDS,
  MAX_WINDOW_DAYS,
  parseReport,
  reportUsage,
  RETENTION_DAYS,
  SIGNATURE_HEADER,
  usageStats,
  windowDays,
} from "../src/usage";
import { APPS, MAX_EVENTS_PER_REPORT } from "../src/validate";
import { d1 } from "./d1sqlite";

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
    // Added to, but never past what a day can hold — see `MAX_DAY_MINUTES`.
    expect(daily.sql).toContain(`sessions = MIN(${MAX_DAY_SESSIONS}, sessions + excluded.sessions)`);
    expect(daily.sql).toContain(`minutes = MIN(${MAX_DAY_MINUTES}, minutes + excluded.minutes)`);
    const event = db.statements.find((s) => s.sql.includes("INTO usage_events"))!;
    expect(event.sql).toContain("count = count + excluded.count");
  });

  it("keys the row on the install, the app and the UTC day, and stores nothing else about the caller", async () => {
    const db = stubDb();
    await reportUsage(post(body()), { DB: db } as unknown as Env);

    const daily = db.statements.find((s) => s.sql.includes("INTO usage_daily"))!;
    expect(daily.args[0]).toBe(INSTALL);
    // The two apps share a config file and so share an install id: without the app in the
    // key they would overwrite each other's version and add up each other's minutes.
    expect(daily.args[1]).toBe("manager");
    expect(daily.args[2]).toBe(new Date().toISOString().slice(0, 10));
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

  it("refuses the key in the query, so a leaked URL is not the admin credential", () => {
    expect(adminAllowed(plain, url("?key=s3cret"), { ADMIN_KEY: "s3cret" } as Env)).toBe("denied");
  });

  it("takes a bearer token, the only way in for anything scripting it", () => {
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
    expect(windowDays(new URL("https://cp.test/admin/usage?days=99999"))).toBe(MAX_WINDOW_DAYS);
    expect(windowDays(new URL("https://cp.test/admin/usage?days=-4"))).toBe(1);
    expect(windowDays(new URL("https://cp.test/admin/usage?days=nonsense"))).toBe(30);
  });

  it("stops short of the prune, which is what keeps `newInstalls` honest", () => {
    // `newInstalls` counts installs whose first day ever falls in the window, and the sweep
    // deletes first days. A window that reached as far back as the prune would report every
    // surviving install as new — no error, just a wrong number. These two have always been
    // related; this is the line that says so.
    expect(MAX_WINDOW_DAYS).toBeLessThan(RETENTION_DAYS);
  });
});

describe("which app a report is from", () => {
  it("is the manager when a build predates the field", () => {
    const report = parseReport(JSON.stringify(body()));
    expect(typeof report === "string" ? report : report.app).toBe("manager");
  });

  it("takes the two it knows and refuses anything else", () => {
    expect(parseReport(JSON.stringify(body({ app: "studio" })))).toMatchObject({ app: "studio" });
    expect(parseReport(JSON.stringify(body({ app: "something" })))).toMatch(/app must be one of/);
  });
});

describe("reading it back", () => {
  it("survives an empty database, which is what the first day looks like", async () => {
    const stats = await collectStats({ DB: stubDb() } as unknown as Env, 30);

    expect(stats.active).toEqual({ day: 0, week: 0, month: 0, window: 0 });
    expect(stats.events).toEqual([]);
    expect(stats.currentVersions).toEqual([]);
    // Everything the app can report is listed as untouched, rather than the page being blank.
    expect(stats.unused).toContain("view.browse");
  });

  it("counts each install under one version: the one it last reported", async () => {
    // Two installs on 0.13.5, one of which was on 0.12.6 earlier in the window. Only the
    // build each is on now says how many would be affected by dropping support for one.
    const db = answering({
      "PARTITION BY install_id": [{ label: "0.13.5", installs: 2 }],
    });
    const stats = await collectStats({ DB: db } as unknown as Env, 30);

    expect(stats.currentVersions).toEqual([{ label: "0.13.5", installs: 2 }]);
  });

  it("compares the window with the one before it", async () => {
    const db = answering({ "MAX(day >= ?1)": [{ recent: 120, prior: 100, returning: 80 }] });
    const stats = await collectStats({ DB: db } as unknown as Env, 30);

    expect(stats.retention).toEqual({ recent: 120, prior: 100, returning: 80 });
    // 60 days of rows read once, not two windows fetched and intersected here.
    const sql = db.asked.find((q) => q.includes("MAX(day >= ?1)"))!;
    expect(sql).toContain("GROUP BY install_id");
  });

  it("leaves retention out rather than guessing when the prune has eaten the window before", async () => {
    const db = answering({});
    const stats = await collectStats({ DB: db } as unknown as Env, MAX_WINDOW_DAYS);

    // A year against the year before it needs 730 days of history and there are 400.
    expect(MAX_WINDOW_DAYS * 2).toBeGreaterThan(RETENTION_DAYS);
    expect(stats.retention).toBeNull();
    expect(db.asked.some((q) => q.includes("MAX(day >= ?1)"))).toBe(false);
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

describe("what a report has to arrive as", () => {
  /**
   * The vector this closes.
   *
   * A `text/plain` POST is a CORS simple request: a web page can have every visitor deliver one
   * from their own address, and does not care that it cannot read the reply. The per-address cap
   * is no answer to that — each visitor brings a fresh address. Insisting on a content type that
   * needs a preflight, on a route that answers no CORS headers, is.
   */
  it("refuses a body that did not declare itself as JSON", async () => {
    const db = stubDb();
    const req = new Request("https://cp.test/v1/usage", {
      method: "POST",
      headers: { "content-type": "text/plain;charset=UTF-8" },
      body: JSON.stringify(body()),
    });

    expect((await reportUsage(req, { DB: db } as unknown as Env)).status).toBe(415);
    expect(db.statements).toHaveLength(0);
  });

  it("takes the type with a charset on it, which is what a client actually sends", async () => {
    const db = stubDb();
    const req = new Request("https://cp.test/v1/usage", {
      method: "POST",
      headers: { "content-type": "application/json; charset=utf-8" },
      body: JSON.stringify(body()),
    });

    expect((await reportUsage(req, { DB: db } as unknown as Env)).status).toBe(202);
  });
});

describe("the ceilings on a day's row", () => {
  it("clamps what one report may add, so a day cannot hold more than a day", async () => {
    const db = stubDb();
    await reportUsage(
      post(body({ sessions: 1000, minutes: 1440 })),
      { DB: db } as unknown as Env,
    );

    const daily = db.statements.find((s) => s.sql.includes("INTO usage_daily"))!;
    expect(daily.args[6]).toBeLessThanOrEqual(MAX_DAY_SESSIONS);
    expect(daily.args[7]).toBeLessThanOrEqual(MAX_DAY_MINUTES);
  });

  /** The one that matters: the row, not the report. Reports accumulate onto it. */
  it("stops the row climbing however many reports land on it", async () => {
    const db = d1();
    const env = { DB: db } as unknown as Env;
    // Every one of these is a report the endpoint would accept on its own terms.
    for (let i = 0; i < 50; i++) {
      await reportUsage(post(body({ sessions: 1000, minutes: 1440, events: [] })), env);
    }

    const row = await db
      .prepare("SELECT sessions, minutes FROM usage_daily WHERE install_id = ?")
      .bind(INSTALL)
      .first<{ sessions: number; minutes: number }>();
    expect(row!.minutes).toBe(MAX_DAY_MINUTES);
    expect(row!.sessions).toBe(MAX_DAY_SESSIONS);
  });
});

describe("a signed report", () => {
  const KEY = "a-build-key";

  async function sign(raw: string, seconds: number): Promise<string> {
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(KEY),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(`v1.${seconds}.${raw}`));
    const hex = [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
    return `v1 ${seconds} ${hex}`;
  }

  function signed(raw: string, header: string): Request {
    return new Request("https://cp.test/v1/usage", {
      method: "POST",
      headers: { "content-type": "application/json", [SIGNATURE_HEADER]: header },
      body: raw,
    });
  }

  /** The default, and the only safe one until signed builds are the ones in the field. */
  it("is not asked for unless the deployment asks for it", async () => {
    const db = stubDb();
    const res = await reportUsage(post(body()), { DB: db, USAGE_SIGNING_KEY: KEY } as unknown as Env);

    expect(res.status).toBe(202);
  });

  it("is refused when it is required and absent", async () => {
    const db = stubDb();
    const env = { DB: db, USAGE_SIGNING_KEY: KEY, MXB_USAGE_REQUIRE_SIGNATURE: "1" } as unknown as Env;

    expect((await reportUsage(post(body()), env)).status).toBe(401);
    expect(db.statements).toHaveLength(0);
  });

  it("is taken when it is required and right", async () => {
    const db = stubDb();
    const env = { DB: db, USAGE_SIGNING_KEY: KEY, MXB_USAGE_REQUIRE_SIGNATURE: "1" } as unknown as Env;
    const raw = JSON.stringify(body());
    const res = await reportUsage(signed(raw, await sign(raw, Math.floor(Date.now() / 1000))), env);

    expect(res.status).toBe(202);
  });

  it("is refused when the body it signed is not the body that arrived", async () => {
    const db = stubDb();
    const env = { DB: db, USAGE_SIGNING_KEY: KEY, MXB_USAGE_REQUIRE_SIGNATURE: "1" } as unknown as Env;
    const header = await sign(JSON.stringify(body()), Math.floor(Date.now() / 1000));
    const tampered = JSON.stringify(body({ sessions: 999 }));

    expect((await reportUsage(signed(tampered, header), env)).status).toBe(401);
  });

  /**
   * The vector `crates/core/src/usage.rs` also checks.
   *
   * The construction is written twice, in two languages, and joined by nothing but agreement.
   * If they drift, a signed build reports nothing the moment a deployment requires a signature,
   * and the only sign of it is numbers quietly going flat.
   */
  it("is rebuilt the way the client builds it", async () => {
    expect(await sign('{"installId":"x"}', 1_700_000_000)).toBe(
      "v1 1700000000 78626f503fcdc861e76c223f96c36373bdd848c8e9aeacca28ee27d41e40db86",
    );
  });

  /** What bounds replay: a captured report is good for the skew window and no longer. */
  it("is refused once its clock is further out than the skew allows", async () => {
    const db = stubDb();
    const env = { DB: db, USAGE_SIGNING_KEY: KEY, MXB_USAGE_REQUIRE_SIGNATURE: "1" } as unknown as Env;
    const raw = JSON.stringify(body());
    const stale = Math.floor(Date.now() / 1000) - MAX_SIGNATURE_SKEW_SECONDS - 60;

    expect((await reportUsage(signed(raw, await sign(raw, stale)), env)).status).toBe(401);
  });
});

describe("counting installs rather than rows", () => {
  const OTHER = "7a2e3c4d-5b6f-4a7e-8f9a-0b1c2d3e4f50";

  /** A row per app per day, so `COUNT(*)` was counting apps and calling them installs. */
  async function twoAppsOneMachine(db: Env["DB"], day: string): Promise<void> {
    for (const [install, app] of [
      [INSTALL, "manager"],
      [INSTALL, "studio"],
      [OTHER, "manager"],
    ] as const) {
      await db
        .prepare(
          "INSERT INTO usage_daily (install_id, app, day, version, os, game, sessions, minutes, first_seen, updated_at)" +
            " VALUES (?, ?, ?, '0.13.5', 'windows', 'mxb', 1, 10, 0, 0)",
        )
        .bind(install, app, day)
        .run();
    }
  }

  it("draws a machine that runs both apps as one install on the daily chart", async () => {
    const db = d1();
    const now = Date.now();
    await twoAppsOneMachine(db, new Date(now).toISOString().slice(0, 10));
    const stats = await collectStats({ DB: db } as unknown as Env, 30, now);

    // Two machines, three rows. The tile beside the chart has always said two.
    expect(stats.daily.at(-1)!.installs).toBe(2);
    expect(stats.active.day).toBe(2);
  });

  it("counts the window's actives, which is the denominator every other figure needs", async () => {
    const db = d1();
    const now = Date.now();
    const old = new Date(now - 60 * 86_400_000).toISOString().slice(0, 10);
    await twoAppsOneMachine(db, old);
    const stats = await collectStats({ DB: db } as unknown as Env, 90, now);

    // Inside a 90-day window, outside the fixed 30-day month. Dividing the window's sessions
    // by `active.month` is what made "Sessions per install" a different number per range.
    expect(stats.active.window).toBe(2);
    expect(stats.active.month).toBe(0);
  });

  /**
   * "Installs seen" says "over {retentionDays} days" on the page. It used to be `WHERE 1 = 1`,
   * and was only ever that number because the sweep happened to have deleted the rest — so a
   * sweep that failed for a week silently changed what the tile meant.
   */
  it("bounds 'installs seen' by the retention window it claims to be", async () => {
    const db = d1();
    const now = Date.now();
    const rows: [string, number][] = [
      [INSTALL, 10],
      [OTHER, RETENTION_DAYS + 30],
    ];
    for (const [install, back] of rows) {
      await db
        .prepare(
          "INSERT INTO usage_daily (install_id, app, day, version, os, game, sessions, minutes, first_seen, updated_at)" +
            " VALUES (?, 'manager', ?, '0.13.5', 'windows', 'mxb', 1, 10, 0, 0)",
        )
        .bind(install, new Date(now - back * 86_400_000).toISOString().slice(0, 10))
        .run();
    }
    const stats = await collectStats({ DB: db } as unknown as Env, 30, now);

    // The row past the prune is still on disk — the sweep has not run — and is still not counted.
    expect(stats.installsEver).toBe(1);
  });
});

describe("the vocabulary", () => {
  it("only offers a name to the app that can send it", () => {
    const studio = knownFor("studio");

    expect(studio).toContain("view.studio.paints");
    // The manager's pages are not the studio's silence.
    expect(studio).not.toContain("view.browse");
    expect(studio).not.toContain("game.launch");
    expect(knownFor("all")).toEqual(Object.keys(KNOWN_EVENTS));
  });

  it("names an app that reports", () => {
    for (const [name, apps] of Object.entries(KNOWN_EVENTS)) {
      expect(apps.length, `${name} is reported by nothing`).toBeGreaterThan(0);
      for (const app of apps) expect(APPS).toContain(app);
    }
  });

  /**
   * The two halves of this feature are written in two languages and held together by nothing
   * but agreement. A name the apps send that this list has never heard of cannot turn up in
   * "Never touched" — which is the one panel whose whole job is to name an absence — so the
   * drift is invisible in exactly the place it matters.
   */
  it("says the same thing the client does", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const rust = readFileSync(join(here, "..", "..", "crates", "core", "src", "usage.rs"), "utf8");
    const block = /pub const KNOWN_EVENTS: &\[&str\] = &\[([\s\S]*?)\];/.exec(rust);
    expect(block, "the client's KNOWN_EVENTS is not where this test expects it").not.toBeNull();

    // Comments out first: a quoted phrase in a comment beside the list is prose, not a name,
    // and reading one as an event made this test fail for a reason that had nothing to do with
    // the drift it exists to catch.
    const names = block![1].replace(/\/\/[^\n]*/g, "");
    const client = [...names.matchAll(/"([^"]+)"/g)].map((m) => m[1]).sort();
    expect(client.length).toBeGreaterThan(0);
    expect(client).toEqual(Object.keys(KNOWN_EVENTS).sort());
  });
});

describe("the Steam bit an install reports", () => {
  const INSTALL_B = "7a2e3c4d-5b6f-4a7e-8f9a-0b1c2d3e4f51";

  /** What a build older than the field sends: nothing. */
  it("reads a report with no steam field as unknown, never as no", () => {
    expect(parseReport(JSON.stringify(body()))).toMatchObject({ steam: "unknown" });
  });

  it("takes the three states and refuses anything else", () => {
    for (const state of ["yes", "no", "unknown"]) {
      expect(parseReport(JSON.stringify(body({ steam: state })))).toMatchObject({ steam: state });
    }
    expect(parseReport(JSON.stringify(body({ steam: true })))).toBe("steam must be yes, no or unknown");
    expect(parseReport(JSON.stringify(body({ steam: "maybe" })))).toBe("steam must be yes, no or unknown");
  });

  /**
   * The upsert's whole reason for a CASE. An install flushes every half hour, and the startup
   * gate may not have answered by the first one — so 'unknown' routinely arrives after 'yes' on
   * the same day. Letting it win would blank the only figure this field exists to produce.
   */
  it("never lets a later unknown erase a sign-in already recorded today", async () => {
    const db = d1();
    const env = { DB: db } as unknown as Env;
    await reportUsage(post(body({ steam: "yes" })), env);
    await reportUsage(post(body({ steam: "unknown" })), env);

    const row = await db
      .prepare("SELECT steam FROM usage_daily WHERE install_id = ?")
      .bind(INSTALL)
      .first<{ steam: string }>();
    expect(row?.steam).toBe("yes");
  });

  /** A real change of state still has to land: signing out is not the same as not knowing. */
  it("lets a known state replace another known state", async () => {
    const db = d1();
    const env = { DB: db } as unknown as Env;
    await reportUsage(post(body({ steam: "no" })), env);
    await reportUsage(post(body({ steam: "yes" })), env);

    const row = await db
      .prepare("SELECT steam FROM usage_daily WHERE install_id = ?")
      .bind(INSTALL)
      .first<{ steam: string }>();
    expect(row?.steam).toBe("yes");
  });

  it("buckets installs by their most recent day, with unknown kept apart", async () => {
    const db = d1();
    const now = Date.now();
    const env = { DB: db } as unknown as Env;
    await reportUsage(post(body({ steam: "yes" })), env);
    await reportUsage(post(body({ installId: INSTALL_B, steam: "no" })), env);
    // A third install on a build that predates the field, sending nothing.
    await reportUsage(post(body({ installId: "7a2e3c4d-5b6f-4a7e-8f9a-0b1c2d3e4f52" })), env);

    const stats = await collectStats(env, 30, now);

    expect(stats.steamInstalls).toEqual({ yes: 1, no: 1, unknown: 1 });
    // The three buckets are the window's installs, so the page can show them as a whole.
    expect(stats.steamInstalls.yes + stats.steamInstalls.no + stats.steamInstalls.unknown).toBe(
      stats.active.window,
    );
  });
});
