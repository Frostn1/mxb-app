import { describe, expect, it } from "vitest";
import { d1 } from "./d1sqlite";
import {
  crashDetail,
  crashSites,
  isSite,
  parseCrash,
  putCrash,
  recentCrashes,
  type Account,
} from "../src/crashes";

const ACCOUNT: Account = { id: "acc-1", rider_name: "Frost", guid: null };

/** A report shaped the way FrostMod writes it. */
function report(over: Record<string, unknown> = {}) {
  return {
    schema: 1,
    frostmod: "0.29.0",
    game: "mxbikes.exe",
    when: "2026-09-16T13:59:29Z",
    uptimeMs: 2_400_000,
    dump: "frostmod-crash-20260916-145929.dmp",
    fault: {
      kind: "access violation",
      code: "0xC0000005",
      site: "mxbikes.exe+0x11D753",
      access: "reading",
      target: "0x0000000000000010",
    },
    frames: ["mxbikes.exe+0x11D753", "mxbikes.exe+0x12782D"],
    session: {
      where: "on track",
      inSession: true,
      track: "Northgate Raceway",
      server: "MXB App Public",
      rider: "Frost",
      riders: 17,
      reloads: 0,
      sinceFrameMs: 12,
      sinceReloadMs: null,
    },
    trail: [{ beforeMs: 1500, text: "rider #14 joined (17 in the session)" }],
    ...over,
  };
}

/** An account row is a foreign key for every crash, and carries columns we do not use. */
async function addAccount(DB: ReturnType<typeof d1>, id: string, riderName: string) {
  await DB.prepare(
    "INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, ?)",
  )
    .bind(id, riderName, `hash-${id}`, Date.now())
    .run();
}

function put(body: unknown) {
  return new Request("https://api.test/v1/diagnostics/crash", {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

describe("a crash site", () => {
  it("takes module+offset, which is what two machines share", () => {
    expect(isSite("mxbikes.exe+0x11D753")).toBe(true);
    expect(isSite("MSVCR90.dll+0x36EDE")).toBe(true);
    expect(isSite("frostmod.dll+0x9a1")).toBe(true);
  });

  it("takes a bare address, because a fault outside every module has no module", () => {
    expect(isSite("0x00007FFD3FA6A0F0")).toBe(true);
  });

  it("refuses anything else, because it is the key everything groups by", () => {
    expect(isSite("mxbikes.exe")).toBe(false);
    expect(isSite("mxbikes.exe+11D753")).toBe(false);
    expect(isSite("../../etc/passwd+0x1")).toBe(false);
    expect(isSite("mxbikes.exe+0x" + "f".repeat(20))).toBe(false);
    expect(isSite("")).toBe(false);
    expect(isSite(42)).toBe(false);
  });
});

describe("reading a report off a client", () => {
  it("takes one FrostMod wrote", () => {
    const parsed = parseCrash(report());
    expect(parsed?.site).toBe("mxbikes.exe+0x11D753");
    expect(parsed?.riders).toBe(17);
    expect(parsed?.inSession).toBe(true);
    expect(parsed?.frames).toHaveLength(2);
    expect(parsed?.trail[0]?.text).toContain("rider #14 joined");
    expect(parsed?.hasDump).toBe(true);
    expect(parsed?.crashedAt).toBe(Date.parse("2026-09-16T13:59:29Z"));
  });

  it("refuses one with no usable site", () => {
    expect(parseCrash(report({ fault: { site: "not a site" } }))).toBeNull();
    expect(parseCrash({})).toBeNull();
    expect(parseCrash(null)).toBeNull();
  });

  it("keeps null apart from zero", () => {
    const parsed = parseCrash(
      report({ session: { riders: null, sinceFrameMs: null, sinceReloadMs: null } }),
    );
    // Not in a race is not a grid of nobody, and a crash before the first frame is not a
    // crash the instant after one.
    expect(parsed?.riders).toBeNull();
    expect(parsed?.sinceFrameMs).toBeNull();
    expect(parsed?.sinceReloadMs).toBeNull();
    expect(parsed?.reloads).toBe(0);
  });

  it("does not take the client's word on when", () => {
    const future = parseCrash(report({ when: "2099-01-01T00:00:00Z" }));
    // A report stamped in the future would sort above every real crash forever.
    expect(future!.crashedAt).toBeLessThanOrEqual(Date.now() + 60_000);
    const nonsense = parseCrash(report({ when: "last tuesday" }));
    expect(nonsense!.crashedAt).toBeGreaterThan(0);
  });

  it("bounds everything a hostile client could make big", () => {
    const parsed = parseCrash(
      report({
        frames: Array.from({ length: 500 }, (_, i) => `mxbikes.exe+0x${i}`),
        trail: Array.from({ length: 500 }, () => ({ beforeMs: 1, text: "x" })),
        session: { server: "s".repeat(5_000), riders: 1e12, reloads: -5 },
        uptimeMs: Number.MAX_SAFE_INTEGER,
      }),
    );
    expect(parsed!.frames.length).toBeLessThanOrEqual(40);
    expect(parsed!.trail.length).toBeLessThanOrEqual(32);
    expect(parsed!.server.length).toBeLessThanOrEqual(160);
    expect(parsed!.riders).toBeLessThanOrEqual(256);
    expect(parsed!.reloads).toBe(0);
    expect(Number.isSafeInteger(parsed!.uptimeMs)).toBe(true);
  });

  it("drops trail entries that are not notes, and keeps the rest", () => {
    const parsed = parseCrash(report({ trail: [null, 7, { beforeMs: 5 }, { text: "kept" }] }));
    expect(parsed!.trail).toEqual([{ beforeMs: 0, text: "kept" }]);
  });
});

describe("storing one", () => {
  it("keeps what the dashboard reads back", async () => {
    const DB = d1();
    await addAccount(DB, ACCOUNT.id, ACCOUNT.rider_name);

    const res = await putCrash(put(report()), ACCOUNT, { DB });
    expect(res.status).toBe(200);

    const recent = await recentCrashes(DB);
    expect(recent).toHaveLength(1);
    expect(recent[0]).toMatchObject({
      riderName: "Frost",
      site: "mxbikes.exe+0x11D753",
      place: "on track",
      track: "Northgate Raceway",
      riders: 17,
      hasDump: true,
    });

    const detail = await crashDetail(DB, "mxbikes.exe+0x11D753");
    expect(detail!.reports[0]!.frames).toEqual([
      "mxbikes.exe+0x11D753",
      "mxbikes.exe+0x12782D",
    ]);
    expect(detail!.reports[0]!.trail).toHaveLength(1);
  });

  it("counts the same crash once, however many times it is sent", async () => {
    const DB = d1();
    await addAccount(DB, ACCOUNT.id, ACCOUNT.rider_name);

    // The app renames its file only on success, so a success whose answer was lost is
    // retried. Twice in the table would make the crash look twice as common as it is.
    await putCrash(put(report()), ACCOUNT, { DB });
    await putCrash(put(report()), ACCOUNT, { DB });
    expect(await recentCrashes(DB)).toHaveLength(1);
  });

  it("answers ok to a report it already had, so the client stops retrying", async () => {
    const DB = d1();
    await addAccount(DB, ACCOUNT.id, ACCOUNT.rider_name);
    await putCrash(put(report()), ACCOUNT, { DB });
    const again = await putCrash(put(report()), ACCOUNT, { DB });
    expect(again.status).toBe(200);
    expect(await again.json()).toEqual({ ok: true });
  });

  it("refuses a body that is not a report", async () => {
    const DB = d1();
    const res = await putCrash(put({ fault: { site: "nope" } }), ACCOUNT, { DB });
    expect(res.status).toBe(400);
  });
});

describe("ranking what to fix", () => {
  it("ranks by how many riders hit it, not by how many times", async () => {
    const DB = d1();
    for (const id of ["a", "b", "c"]) await addAccount(DB, id, `rider-${id}`);

    // One rider having a bad night: six reports of the same fault.
    for (let i = 0; i < 6; i++) {
      await putCrash(
        put(report({ when: `2026-09-1${i}T10:00:00Z`, fault: { site: "mxbikes.exe+0xAAA" } })),
        { id: "a", rider_name: "rider-a" },
        { DB },
      );
    }
    // Three riders hitting the same thing: that is a bug, and it should rank above.
    for (const id of ["a", "b", "c"]) {
      await putCrash(
        put(report({ fault: { site: "mxbikes.exe+0xBBB" } })),
        { id, rider_name: `rider-${id}` },
        { DB },
      );
    }

    const sites = await crashSites(DB);
    expect(sites[0]!.site).toBe("mxbikes.exe+0xBBB");
    expect(sites[0]!.riders).toBe(3);
    expect(sites[1]!.site).toBe("mxbikes.exe+0xAAA");
    expect(sites[1]!.hits).toBe(6);
    expect(sites[1]!.riders).toBe(1);
  });

  it("says nothing about a site nobody has reported", async () => {
    const DB = d1();
    expect(await crashDetail(DB, "mxbikes.exe+0x1")).toEqual({
      site: "mxbikes.exe+0x1",
      reports: [],
    });
    expect(await crashDetail(DB, "garbage")).toBeNull();
  });
});
