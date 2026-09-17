import { describe, expect, it } from "vitest";
import { rememberLink, repairBySteamId, steamIdFor } from "../src/steamlink";
import { addAccount, d1 } from "./d1sqlite";

const FROST = "76561199164505734";
const OTHER = "76561198174305985";

async function deployment(): Promise<Env> {
  const DB = d1();
  await addAccount(DB, "acc_frost", "Frost");
  await addAccount(DB, "acc_nico", "nico_maier");
  return { DB } as unknown as Env;
}

/** The account row as it stands, which is the thing a restore has to actually change. */
async function steamIdOf(env: Env, id: string): Promise<string | null> {
  const row = await env.DB.prepare("SELECT steam_id FROM accounts WHERE id = ?")
    .bind(id)
    .first<{ steam_id: string | null }>();
  return row?.steam_id ?? null;
}

async function link(env: Env, accountId: string, steamId: string, alsoSetColumn = true): Promise<void> {
  await rememberLink(env, accountId, steamId).run();
  if (alsoSetColumn) {
    await env.DB.prepare("UPDATE accounts SET steam_id = ? WHERE id = ?").bind(steamId, accountId).run();
  }
}

describe("steamIdFor", () => {
  it("uses the column when it is there", async () => {
    const env = await deployment();
    await link(env, "acc_frost", FROST);
    expect(await steamIdFor(env, { id: "acc_frost", steam_id: FROST })).toBe(FROST);
  });

  it("is null for an account that never linked, and writes nothing", async () => {
    const env = await deployment();
    expect(await steamIdFor(env, { id: "acc_nico", steam_id: null })).toBeNull();
    expect(await steamIdOf(env, "acc_nico")).toBeNull();
  });

  it("puts back a link the column has lost", async () => {
    const env = await deployment();
    await link(env, "acc_frost", FROST);
    // What happened in production: the column cleared, the confirmed link still true.
    await env.DB.prepare("UPDATE accounts SET steam_id = NULL WHERE id = 'acc_frost'").run();

    expect(await steamIdFor(env, { id: "acc_frost", steam_id: null })).toBe(FROST);
    // Restored on the row, not just in the answer — the next caller must not have to repair again.
    expect(await steamIdOf(env, "acc_frost")).toBe(FROST);
  });

  it("will not take a column another account now holds, but still names the identity", async () => {
    const env = await deployment();
    await link(env, "acc_frost", FROST);
    await env.DB.prepare("UPDATE accounts SET steam_id = NULL WHERE id = 'acc_frost'").run();
    // Another account holds the value now — in practice a second install of the same person.
    // Which row keeps the unique cell is not a question to answer unattended, so the column is
    // left exactly as it is. What this account's identity *is* was never in doubt: Valve
    // confirmed the link and the log still says so. Refusing to say it is what used to leave a
    // second machine unable to open any app in the lineup.
    await link(env, "acc_nico", FROST);

    expect(await steamIdFor(env, { id: "acc_frost", steam_id: null })).toBe(FROST);
    expect(await steamIdOf(env, "acc_frost")).toBeNull();
    expect(await steamIdOf(env, "acc_nico")).toBe(FROST);
  });

  it("restores the most recent identity when an account has linked more than one", async () => {
    const env = await deployment();
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES ('acc_frost', ?, 1000)")
      .bind(OTHER)
      .run();
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES ('acc_frost', ?, 2000)")
      .bind(FROST)
      .run();

    expect(await steamIdFor(env, { id: "acc_frost", steam_id: null })).toBe(FROST);
  });
});

describe("repairBySteamId", () => {
  it("finds the account behind an identity the column has lost", async () => {
    const env = await deployment();
    await link(env, "acc_nico", OTHER);
    await env.DB.prepare("UPDATE accounts SET steam_id = NULL WHERE id = 'acc_nico'").run();

    expect(await repairBySteamId(env, OTHER)).toBe(true);
    expect(await steamIdOf(env, "acc_nico")).toBe(OTHER);
  });

  it("is false for an identity nobody has ever linked", async () => {
    const env = await deployment();
    expect(await repairBySteamId(env, OTHER)).toBe(false);
  });

  it("is false for something that is not a SteamID64, without asking the database", async () => {
    const env = await deployment();
    expect(await repairBySteamId(env, "unlinked")).toBe(false);
    expect(await repairBySteamId(env, "'; DROP TABLE accounts; --")).toBe(false);
  });

  it("leaves a column that is already set alone", async () => {
    const env = await deployment();
    await link(env, "acc_frost", FROST);
    expect(await repairBySteamId(env, FROST)).toBe(false);
    expect(await steamIdOf(env, "acc_frost")).toBe(FROST);
  });
});

describe("rememberLink", () => {
  it("keeps one row per pair and moves the time on a re-link", async () => {
    const env = await deployment();
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES ('acc_frost', ?, 1000)")
      .bind(FROST)
      .run();
    await rememberLink(env, "acc_frost", FROST).run();

    const rows = await env.DB.prepare("SELECT linked_at FROM steam_links WHERE account_id = 'acc_frost'").all<{
      linked_at: number;
    }>();
    expect(rows.results).toHaveLength(1);
    expect(rows.results[0].linked_at).toBeGreaterThan(1000);
  });
});
