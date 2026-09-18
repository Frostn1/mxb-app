import { describe, expect, it } from "vitest";
import {
  LOGIN_START_TTL_MS,
  LOGIN_TTL_MS,
  markRefused,
  markStarted,
  mayReturn,
  mayStart,
  pageResult,
  type PendingLogin,
} from "../src/signin";
import { addAccount, d1 } from "./d1sqlite";

const T0 = 1_700_000_000_000;
const MINUTE = 60 * 1000;

function pending(over: Partial<PendingLogin> = {}): PendingLogin {
  return { created_at: T0, started_at: null, consumed_at: null, ...over };
}

describe("mayStart", () => {
  it("lets a fresh login through to Steam", () => {
    expect(mayStart(pending(), T0 + MINUTE)).toEqual({ ok: true });
  });

  it("refuses a login that isn't there", () => {
    expect(mayStart(null, T0)).toEqual({ ok: false, reason: "unknown" });
  });

  it("refuses one that has already been through Valve", () => {
    expect(mayStart(pending({ consumed_at: T0 + MINUTE }), T0 + 2 * MINUTE)).toEqual({
      ok: false,
      reason: "spent",
    });
  });

  it("refuses a URL that sat unopened past the start window", () => {
    expect(mayStart(pending(), T0 + LOGIN_START_TTL_MS + 1)).toEqual({
      ok: false,
      reason: "never-opened",
    });
  });

  it("still opens one at the very edge of the start window", () => {
    expect(mayStart(pending(), T0 + LOGIN_START_TTL_MS)).toEqual({ ok: true });
  });
});

describe("mayReturn", () => {
  it("counts the window from the browser arriving, not from the mint", () => {
    // The app minted this an hour ago and the browser only reached Steam a minute back. Under
    // the old rule — one ten-minute deadline counted from `created_at` — this was the refusal
    // that landed *after* the rider had signed in, and it is the whole point of the change.
    const login = pending({ started_at: T0 + 60 * MINUTE });
    expect(mayReturn(login, T0 + 61 * MINUTE)).toEqual({ ok: true });
  });

  it("refuses once the rider's own window has run out", () => {
    const login = pending({ started_at: T0 });
    expect(mayReturn(login, T0 + LOGIN_TTL_MS + 1)).toEqual({ ok: false, reason: "timed-out" });
  });

  it("allows the slow tail the old ten minutes refused", () => {
    const login = pending({ started_at: T0 });
    expect(mayReturn(login, T0 + 20 * MINUTE)).toEqual({ ok: true });
  });

  it("falls back to the mint for a return that never came through the start page", () => {
    expect(mayReturn(pending(), T0 + MINUTE)).toEqual({ ok: true });
    expect(mayReturn(pending(), T0 + LOGIN_TTL_MS + 1)).toEqual({ ok: false, reason: "timed-out" });
  });

  it("refuses a spent login before it looks at the clock", () => {
    const login = pending({ started_at: T0, consumed_at: T0 + MINUTE });
    expect(mayReturn(login, T0 + 2 * MINUTE)).toEqual({ ok: false, reason: "spent" });
  });

  it("refuses a login that isn't there", () => {
    expect(mayReturn(null, T0)).toEqual({ ok: false, reason: "unknown" });
  });
});

describe("pageResult", () => {
  it("says 'expired' for everything that means start it again", () => {
    expect(pageResult("unknown")).toBe("expired");
    expect(pageResult("spent")).toBe("expired");
    expect(pageResult("never-opened")).toBe("expired");
    expect(pageResult("timed-out")).toBe("expired");
  });

  it("keeps what happened at Steam as its own answer", () => {
    expect(pageResult("unconfirmed")).toBe("unconfirmed");
    // Recorded apart from `unconfirmed`, shown the same: "that sign-in expired" is a lie told
    // to somebody who pressed Cancel a second ago.
    expect(pageResult("cancelled")).toBe("unconfirmed");
  });
});

/** A deployment with one account and one pending login on it. */
async function deployment(): Promise<{ env: Env; loginId: string }> {
  const DB = d1();
  await addAccount(DB, "acc_rider", "Rider");
  const loginId = "login-1";
  await DB.prepare("INSERT INTO steam_logins (id, account_id, created_at) VALUES (?, ?, ?)")
    .bind(loginId, "acc_rider", T0)
    .run();
  return { env: { DB } as unknown as Env, loginId };
}

function read(env: Env, loginId: string): Promise<PendingLogin & { reason: string | null; refused_at: number | null }> {
  return env.DB.prepare(
    "SELECT created_at, started_at, consumed_at, refused_at, reason FROM steam_logins WHERE id = ?",
  )
    .bind(loginId)
    .first() as Promise<PendingLogin & { reason: string | null; refused_at: number | null }>;
}

describe("markStarted", () => {
  it("stamps the moment the browser arrived", async () => {
    const { env, loginId } = await deployment();
    await markStarted(env, loginId, T0 + MINUTE);
    expect((await read(env, loginId)).started_at).toBe(T0 + MINUTE);
  });

  it("stamps once, so reloading the start page cannot roll the deadline forward", async () => {
    const { env, loginId } = await deployment();
    await markStarted(env, loginId, T0 + MINUTE);
    await markStarted(env, loginId, T0 + 40 * MINUTE);
    expect((await read(env, loginId)).started_at).toBe(T0 + MINUTE);
  });

  it("never throws: this sits on the happy path of every sign-in there is", async () => {
    const broken = {
      DB: {
        prepare: () => {
          throw new Error("D1_ERROR: no");
        },
      },
    } as unknown as Env;
    await expect(markStarted(broken, "login-1", T0)).resolves.toBeUndefined();
  });
});

describe("markRefused", () => {
  it("writes the refusal onto the row", async () => {
    const { env, loginId } = await deployment();
    await markRefused(env, loginId, "timed-out", T0 + 40 * MINUTE);
    const row = await read(env, loginId);
    expect(row.reason).toBe("timed-out");
    expect(row.refused_at).toBe(T0 + 40 * MINUTE);
  });

  it("does not consume the login — a refusal is not a completed sign-in", async () => {
    const { env, loginId } = await deployment();
    await markRefused(env, loginId, "unconfirmed", T0 + MINUTE);
    expect((await read(env, loginId)).consumed_at).toBeNull();
  });

  it("is quiet about a login that isn't there: bookkeeping never becomes a refusal", async () => {
    const { env } = await deployment();
    await expect(markRefused(env, "no-such-login", "unknown", T0)).resolves.toBeUndefined();
  });
});
