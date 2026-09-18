import { describe, expect, it } from "vitest";
import { compareVersions, steamAdoption } from "../src/steamstats";
import { d1 } from "./d1sqlite";

/** An account, and the `client_modules` row that says which build it reported from. */
async function seen(
  db: Env["DB"],
  id: string,
  version: string,
  steamId: string | null,
  daysBack = 0,
  now = Date.now(),
): Promise<void> {
  await db
    .prepare("INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, 0)")
    .bind(id, id, steamId, `hash-${id}`)
    .run();
  await db
    .prepare(
      "INSERT INTO client_modules (account_id, state, rules_version, module_count, unknown_count," +
        " matched, worst_state, worst_at, app_version, updated_at)" +
        " VALUES (?, 'ok', 1, 10, 0, '[]', 'ok', 0, ?, ?)",
    )
    .bind(id, version, now - daysBack * 86_400_000)
    .run();
}

describe("ordering the builds", () => {
  it("reads the numbers as numbers, not as text", () => {
    // The bug this exists to prevent: as strings, "0.9.0" sorts above "0.17.0", which would
    // hand "latest" to a build from months ago and make the headline figure quietly wrong.
    expect(compareVersions("0.17.0", "0.9.0")).toBeGreaterThan(0);
    expect(compareVersions("0.17.2", "0.17.10")).toBeLessThan(0);
    expect(compareVersions("1.0.0", "0.99.99")).toBeGreaterThan(0);
    expect(compareVersions("0.17.2", "0.17.2")).toBe(0);
  });

  it("puts a release above its own pre-release", () => {
    expect(compareVersions("0.1.16", "0.1.16-beta.16")).toBeGreaterThan(0);
    expect(compareVersions("0.1.16-beta.2", "0.1.16-beta.10")).toBeGreaterThan(0);
  });
});

describe("the Steam split", () => {
  it("says nothing rather than something wrong when nobody has reported", async () => {
    const stats = await steamAdoption({ DB: d1() } as unknown as Env, 30);

    expect(stats).toEqual({ seen: 0, linked: 0, latest: "", onLatest: 0, onLatestLinked: 0, byVersion: [] });
  });

  it("counts the newest build's accounts and how many of them Valve confirmed", async () => {
    const db = d1();
    const now = Date.now();
    // Three on the new build, two of them linked; two left behind on the old one, neither.
    await seen(db, "a", "0.17.2", "76561198000000001", 0, now);
    await seen(db, "b", "0.17.2", "76561198000000002", 0, now);
    await seen(db, "c", "0.17.2", null, 0, now);
    await seen(db, "d", "0.13.6", null, 1, now);
    await seen(db, "e", "0.13.6", null, 1, now);

    const stats = await steamAdoption({ DB: db } as unknown as Env, 30, now);

    expect(stats.latest).toBe("0.17.2");
    expect(stats.onLatest).toBe(3);
    expect(stats.onLatestLinked).toBe(2);
    // The denominator travels with the figure, because it is not the same population as the
    // install counts sitting beside it on the page.
    expect(stats.seen).toBe(5);
    expect(stats.linked).toBe(2);
    expect(stats.byVersion).toEqual([
      { label: "0.17.2", accounts: 3, linked: 2 },
      { label: "0.13.6", accounts: 2, linked: 0 },
    ]);
  });

  it("moves with the window the rest of the dashboard is drawn from", async () => {
    const db = d1();
    const now = Date.now();
    await seen(db, "recent", "0.17.2", "76561198000000001", 2, now);
    await seen(db, "stale", "0.17.2", "76561198000000002", 40, now);

    expect((await steamAdoption({ DB: db } as unknown as Env, 7, now)).seen).toBe(1);
    expect((await steamAdoption({ DB: db } as unknown as Env, 90, now)).seen).toBe(2);
  });

  it("drops the accounts that never named a build", async () => {
    // `app_version` defaults to empty, and an empty string is not a version — grouping on it
    // would put a bar labelled nothing at the top of the panel the moment it sorted highest.
    const db = d1();
    const now = Date.now();
    await seen(db, "named", "0.17.2", "76561198000000001", 0, now);
    await seen(db, "silent", "", "76561198000000002", 0, now);

    const stats = await steamAdoption({ DB: db } as unknown as Env, 30, now);

    expect(stats.byVersion).toEqual([{ label: "0.17.2", accounts: 1, linked: 1 }]);
    expect(stats.seen).toBe(1);
  });
});
