import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEGRADED_SHARE,
  DOWN_SHARE,
  MAX_PROBES_PER_DAY,
  MIN_INSTALLS,
  RETENTION_MS,
  SWEEP_FRESH_MS,
  WINDOW_MS,
  masterStatus,
  parseProbe,
  pruneMasterProbes,
  reportMasterProbe,
  stateFor,
  headline,
  summarize,
  type StatusBody,
} from "../src/masterstatus";
import { ipDigest } from "../src/voice";
import { d1 } from "./d1sqlite";

let e: Env;
let clock = 1_800_000_000_000;

/** UUIDs that differ only in their last digits, so a test reads as "four installs". */
function install(n: number): string {
  return `7d2f4a1c-0e5b-4c3d-9a6f-${String(n).padStart(12, "0")}`;
}

async function probe(
  n: number,
  ok: boolean,
  reason?: string,
  ip = `203.0.113.${n % 250}`,
): Promise<Response> {
  return reportMasterProbe(
    new Request("https://cp.invalid/v1/master-status", {
      method: "POST",
      headers: { "CF-Connecting-IP": ip },
      body: JSON.stringify({ installId: install(n), ok, ...(reason ? { reason } : {}) }),
    }),
    e,
  );
}

async function status(): Promise<StatusBody> {
  const res = await masterStatus(e);
  expect(res.status).toBe(200);
  return (await res.json()) as StatusBody;
}

/**
 * Plant a successful master sweep, as the roster's snapshot endpoint would have.
 *
 * Only an app whose sweep came back from the master ever writes this row, so its timestamp is
 * the control plane's one piece of positive evidence about PiBoSo's server.
 */
async function swept(at: number): Promise<void> {
  await e.DB.prepare(
    "INSERT INTO server_snapshot (id, payload, servers, updated_at) VALUES ('live', '[]', 0, ?)" +
      " ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at",
  )
    .bind(at)
    .run();
}

beforeEach(() => {
  e = { DB: d1() } as unknown as Env;
  vi.useFakeTimers();
  vi.setSystemTime(clock);
});

afterEach(() => {
  vi.useRealTimers();
  // A fresh day each test, so the per-address cap in one never reaches into the next.
  clock += 24 * 60 * 60 * 1000;
});

describe("what a probe may say", () => {
  it("takes a success with no reason", () => {
    expect(parseProbe(JSON.stringify({ installId: install(1), ok: true }))).toEqual({
      installId: install(1),
      ok: true,
      reason: null,
    });
  });

  it("defaults a failure with no reason to error rather than refusing it", () => {
    expect(parseProbe(JSON.stringify({ installId: install(1), ok: false }))).toEqual({
      installId: install(1),
      ok: false,
      reason: "error",
    });
  });

  it("refuses anything but a UUID as the install id", () => {
    // The point of the shape: a client that decided to send a rider name gets rejected.
    expect(parseProbe(JSON.stringify({ installId: "Liposek", ok: false }))).toBe(
      "installId must be a UUID",
    );
  });

  it("refuses a reason outside the closed list", () => {
    // Free text here would eventually carry an address or a path onto a public page.
    const raw = JSON.stringify({ installId: install(1), ok: false, reason: "192.168.1.1 refused" });
    expect(parseProbe(raw)).toMatch(/^not a probe reason/);
  });

  it("refuses a missing or non-boolean ok", () => {
    expect(parseProbe(JSON.stringify({ installId: install(1) }))).toBe("ok must be a boolean");
    expect(parseProbe(JSON.stringify({ installId: install(1), ok: "yes" }))).toBe(
      "ok must be a boolean",
    );
  });

  it("refuses something that isn't JSON at all", () => {
    expect(parseProbe("not json")).toBe("expected a JSON body");
    expect(parseProbe('"a string"')).toBe("expected a JSON body");
  });
});

describe("where a window lands", () => {
  it("says nothing at all below the floor", () => {
    // The whole value of the number is that it declines to guess at 3am.
    expect(stateFor(MIN_INSTALLS - 1, MIN_INSTALLS - 1)).toBe("unknown");
  });

  it("calls it down when almost everyone is failing", () => {
    expect(stateFor(10, 10)).toBe("down");
    expect(stateFor(10, Math.ceil(DOWN_SHARE * 10))).toBe("down");
  });

  it("calls it degraded between the two thresholds", () => {
    expect(stateFor(10, Math.ceil(DEGRADED_SHARE * 10))).toBe("degraded");
    expect(stateFor(10, Math.ceil(DOWN_SHARE * 10) - 1)).toBe("degraded");
  });

  it("calls it up when one person out of many is failing", () => {
    // The answer Discord actually needs on a quiet day: it is you, not the master.
    expect(stateFor(20, 1)).toBe("up");
  });
});

describe("the sentence", () => {
  it("says whose problem it isn't when the master is down", () => {
    const line = summarize({
      state: "down",
      installs: 25,
      failing: 23,
      share: 0.92,
      reasons: [{ reason: "timeout", installs: 23 }],
      failingForMinutes: 9,
      lastSweep: null,
      local: 0,
    });
    expect(line).toContain("23 of the last 25");
    expect(line).toContain("isn't your connection");
  });

  it("says it probably is yours when everyone else is fine", () => {
    const line = summarize({
      state: "up",
      installs: 25,
      failing: 1,
      share: 0.04,
      reasons: [],
      failingForMinutes: null,
      lastSweep: null,
      local: 0,
    });
    expect(line).toContain("24 of the last 25");
    expect(line).toContain("something at your end");
  });

  it("says whose fault it is when the window is mostly our own failures", () => {
    // The two kinds of "we can't say" must not read alike: one means come back later, the
    // other means update your app. Saying "not enough people have checked" over 164 reports
    // is the version of this that sends people to reinstall a working game.
    const status = {
      state: "unknown" as const,
      installs: 11,
      failing: 9,
      share: 0.82,
      reasons: [{ reason: "timeout" as const, installs: 9 }],
      failingForMinutes: null,
      lastSweep: null,
      local: 153,
    };
    expect(summarize(status)).toContain("never got as far as asking");
    expect(summarize(status)).toContain("fault in the MXB App");
    expect(summarize(status)).not.toContain("Not enough apps");
    expect(headline(status)).toContain("that's ours, not MX Bikes'");
  });

  it("keeps the quiet-hours wording for a genuinely empty window", () => {
    const line = summarize({
      state: "unknown",
      installs: 2,
      failing: 2,
      share: 1,
      reasons: [],
      failingForMinutes: null,
      lastSweep: null,
      local: 0,
    });
    expect(line).toContain("Not enough apps");
  });

  it("admits it doesn't know rather than guessing", () => {
    const line = summarize({
      state: "unknown",
      installs: 1,
      failing: 1,
      share: 1,
      reasons: [],
      failingForMinutes: null,
      lastSweep: null,
      local: 0,
    });
    expect(line).toContain("Not enough apps");
  });
});

describe("what the window is read from", () => {
  it("is unknown with nothing reported", async () => {
    const body = await status();
    expect(body.state).toBe("unknown");
    expect(body.master.installs).toBe(0);
    expect(body.master.share).toBeNull();
  });

  it("counts people, not requests", async () => {
    // One person hammering Refresh must not be able to declare an outage on their own.
    for (let i = 0; i < 30; i += 1) await probe(1, false, "timeout");
    for (let n = 2; n <= 6; n += 1) await probe(n, true);

    const body = await status();
    expect(body.master.installs).toBe(6);
    expect(body.master.failing).toBe(1);
    expect(body.state).toBe("up");
  });

  it("calls an outage when nearly every install is failing", async () => {
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");
    await probe(10, true);

    const body = await status();
    expect(body.state).toBe("down");
    expect(body.master.failing).toBe(9);
    expect(body.master.reasons[0]).toEqual({ reason: "timeout", installs: 9 });
    expect(body.summary).toContain("isn't your connection");
  });

  it("does not count an install that recovered inside the window", async () => {
    // A machine that failed at :02 and succeeded at :04 watched the master come back. Its
    // connection works, and calling it broken would overstate every outage by however many
    // people were mid-refresh when it ended.
    for (let n = 1; n <= 8; n += 1) await probe(n, false, "timeout");
    vi.setSystemTime(clock + 2 * 60_000);
    for (let n = 1; n <= 8; n += 1) await probe(n, true);

    const body = await status();
    expect(body.master.failing).toBe(0);
    expect(body.state).toBe("up");
  });

  it("drops builds with no server browser instead of counting them as an outage", async () => {
    // `unsupported` is a fact about the build, not about the master. Left in, it would peg
    // the ratio high for ever on any install that can't ask in the first place.
    for (let n = 1; n <= 8; n += 1) await probe(n, false, "unsupported");
    for (let n = 9; n <= 12; n += 1) await probe(n, true);

    const body = await status();
    expect(body.master.installs).toBe(4);
    expect(body.master.failing).toBe(0);
    expect(body.state).toBe("up");
  });

  it("drops failures that are facts about the machine, not the master", async () => {
    // The bug this is for: on 2026-09-18 the page read `down` with 162 of 164 apps failing,
    // and 152 of those were our own Steam ticket path breaking. An app that could not mint a
    // ticket, or had no network at all, never asked the master and has observed nothing.
    for (let n = 1; n <= 8; n += 1) await probe(n, false, "ticket");
    for (let n = 9; n <= 10; n += 1) await probe(n, false, "offline");
    for (let n = 11; n <= 14; n += 1) await probe(n, true);

    const body = await status();
    expect(body.master.installs).toBe(4);
    expect(body.master.failing).toBe(0);
    expect(body.state).toBe("up");
  });

  it("will not call an outage over the top of a sweep that just worked", async () => {
    // A real sweep landed a minute ago, so the master answered somebody a minute ago. Whatever
    // everyone else is hitting, it is not PiBoSo's server being down.
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");
    await swept(clock - 60_000);

    const body = await status();
    expect(body.state).toBe("degraded");
    expect(body.master.lastSweep).toBe(clock - 60_000);
    expect(body.summary).toContain("master server itself is answering");
    // Still `down` on the ratio alone — the floor is what moved it, not the count.
    expect(stateFor(body.master.installs, body.master.failing)).toBe("down");
  });

  it("won't accuse anyone on the remnant left after our own bug", async () => {
    // 2026-09-18, in miniature: a ticket regression takes most of the population out, and the
    // few installs that can still ask are survivors rather than a sample. 9 of 11 failing is
    // 82% and cleared the old threshold, so the page published "MX Bikes' servers aren't
    // answering" on the strength of nine machines while the master was serving 69 servers.
    for (let n = 1; n <= 40; n += 1) await probe(n, false, "auth");
    for (let n = 41; n <= 49; n += 1) await probe(n, false, "timeout");
    for (let n = 50; n <= 51; n += 1) await probe(n, true);

    const body = await status();
    expect(body.master.local).toBe(40);
    expect(body.master.installs).toBe(11);
    expect(body.master.failing).toBe(9);
    // The ratio on its own still says down. The sampling guard is what stops it.
    expect(stateFor(11, 9)).toBe("down");
    expect(body.state).toBe("unknown");
  });

  it("still calls a real outage when local failures are a minority", async () => {
    for (let n = 1; n <= 2; n += 1) await probe(n, false, "offline");
    for (let n = 3; n <= 20; n += 1) await probe(n, false, "timeout");

    const body = await status();
    expect(body.master.local).toBe(2);
    expect(body.state).toBe("down");
  });

  it("calls a real outage once the last good sweep goes stale", async () => {
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");
    await swept(clock - SWEEP_FRESH_MS - 1);

    const body = await status();
    expect(body.state).toBe("down");
    expect(body.summary).toContain("isn't your connection");
  });

  it("doesn't quote a sweep from an hour ago", async () => {
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");
    await swept(clock - RETENTION_MS - 60_000);

    const body = await status();
    expect(body.state).toBe("down");
    expect(body.master.lastSweep).toBeNull();
  });

  it("leaves a healthy window alone whether or not a sweep landed", async () => {
    // The floor only ever softens `down`. It must not talk a real degradation up to `up`.
    for (let n = 1; n <= 5; n += 1) await probe(n, false, "timeout");
    for (let n = 6; n <= 10; n += 1) await probe(n, true);
    await swept(clock - 60_000);

    expect((await status()).state).toBe("degraded");
  });

  it("forgets what falls out of the window", async () => {
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");
    expect((await status()).state).toBe("down");

    vi.setSystemTime(clock + WINDOW_MS + 60_000);
    const body = await status();
    expect(body.master.installs).toBe(0);
    expect(body.state).toBe("unknown");
  });

  it("dates an outage from before the window rather than from its edge", async () => {
    // The window is ten minutes; an outage twenty minutes old should read as twenty, or the
    // page would tell everyone every outage started exactly ten minutes ago.
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");
    vi.setSystemTime(clock + 20 * 60_000);
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");

    const body = await status();
    expect(body.state).toBe("down");
    expect(body.master.failingForMinutes).toBeGreaterThanOrEqual(20);
  });

  it("sees an outage that has only just started", async () => {
    // The case the whole feature is for, and the one an "ever succeeded in the window" rule
    // gets exactly backwards: for the first ten minutes every affected install has a success
    // behind it, because it was working right up until the master stopped.
    for (let n = 1; n <= 9; n += 1) await probe(n, true);
    vi.setSystemTime(clock + 5 * 60_000);
    for (let n = 1; n <= 9; n += 1) await probe(n, false, "timeout");

    const body = await status();
    expect(body.state).toBe("down");
    expect(body.master.failing).toBe(9);
    // Dated from the first minute we actually saw fail, not from the last success: nobody
    // reported in between, so claiming it broke five minutes ago would be inventing four of
    // them. The run is one minute old and says so.
    expect(body.master.failingForMinutes).toBe(1);
  });

  it("leaves the duration null when it is only degraded", async () => {
    // A degraded window has a success in every minute by definition, so there is no honest
    // answer to "how long" — better null than a 1 that means nothing.
    for (let n = 1; n <= 5; n += 1) await probe(n, false, "timeout");
    for (let n = 6; n <= 10; n += 1) await probe(n, true);

    const body = await status();
    expect(body.state).toBe("degraded");
    expect(body.master.failingForMinutes).toBeNull();
  });

  it("ranks the reasons so the commonest is first", async () => {
    for (let n = 1; n <= 6; n += 1) await probe(n, false, "timeout");
    for (let n = 7; n <= 8; n += 1) await probe(n, false, "dns");
    await probe(9, false, "refused");

    const body = await status();
    expect(body.master.reasons.map((r) => r.reason)).toEqual(["timeout", "dns", "refused"]);
  });
});

describe("the endpoint itself", () => {
  it("answers a probe with 202 and stores it", async () => {
    const res = await probe(1, false, "timeout");
    expect(res.status).toBe(202);
    const row = await e.DB.prepare("SELECT install_id, failed, reason FROM master_probes")
      .first<{ install_id: string; failed: number; reason: string }>();
    expect(row).toEqual({ install_id: install(1), failed: 1, reason: "timeout" });
  });

  it("refuses a body far larger than a probe could be", async () => {
    const res = await reportMasterProbe(
      new Request("https://cp.invalid/v1/master-status", {
        method: "POST",
        headers: { "content-length": String(64 * 1024) },
        body: JSON.stringify({ installId: install(1), ok: true }),
      }),
      e,
    );
    expect(res.status).toBe(413);
  });

  it("caps one address for the day", async () => {
    const day = new Date(clock).toISOString().slice(0, 10);
    await e.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
        " VALUES (?, ?, 'master', ?, 0)",
    )
      .bind(await ipDigest("203.0.113.9", day, e), day, MAX_PROBES_PER_DAY)
      .run();

    const res = await probe(9, false, "timeout", "203.0.113.9");
    expect(res.status).toBe(429);
  });

  it("is open to anything that wants to read it", async () => {
    // A Discord bot answering `!timeout` and a community site are both meant to.
    const res = await masterStatus(e);
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
    expect(res.headers.get("cache-control")).toContain("max-age");
  });

  it("says how it knows, rather than presenting itself as a probe of the master", async () => {
    const body = await status();
    expect(body.method).toContain("Every MXB App reports");
  });
});

describe("the sweep", () => {
  it("drops what is past retention and keeps what isn't", async () => {
    await probe(1, false, "timeout");
    vi.setSystemTime(clock + RETENTION_MS + 60_000);
    await probe(2, true);

    await pruneMasterProbes(e);
    const rows = await e.DB.prepare("SELECT install_id FROM master_probes").all<{
      install_id: string;
    }>();
    expect(rows.results?.map((r) => r.install_id)).toEqual([install(2)]);
  });
});
