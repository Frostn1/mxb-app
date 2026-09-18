import { describe, expect, it } from "vitest";
import { d1 } from "./d1sqlite";
import { eraseAccount, type Account } from "../src/erasure";

type DB = ReturnType<typeof d1>;

const NOW = Date.now();

async function account(DB: DB, id: string, over: Partial<Record<string, unknown>> = {}) {
  await DB.prepare(
    "INSERT INTO accounts (id, rider_name, steam_id, guid, token_hash, kind, created_at)" +
      " VALUES (?, ?, ?, ?, ?, ?, ?)",
  )
    .bind(
      id,
      (over.rider_name as string) ?? "Frost",
      "steam_id" in over ? (over.steam_id as string | null) : "76561198000000001",
      // `in`, not `??`: an explicit null is the point of the second account in one test, and
      // both columns are uniquely indexed where they are not null.
      "guid" in over ? (over.guid as string | null) : "FF011000013A7C2E91",
      `hash-${id}`,
      (over.kind as string) ?? "invited",
      NOW,
    )
    .run();
}

/** One row in every table erasure is supposed to empty, so a miss shows up as a leftover. */
async function fillPersonal(DB: DB, id: string) {
  await DB.batch([
    DB.prepare("INSERT INTO loadouts (account_id, bike_id, updated_at) VALUES (?, 'mx1', ?)").bind(id, NOW),
    DB.prepare(
      "INSERT INTO loadout_paints (account_id, bike_id, slot, file_name, sha256, size, rel_dest)" +
        " VALUES (?, 'mx1', 'bike', 'frost.pnt', ?, 1024, 'bikes/mx1')",
    ).bind(id, "a".repeat(64)),
    DB.prepare("INSERT INTO presence (account_id, server_id, updated_at) VALUES (?, 'srv-1', ?)").bind(id, NOW),
    DB.prepare(
      "INSERT INTO server_queue (account_id, server_id, joined_at, updated_at) VALUES (?, 'srv-1', ?, ?)",
    ).bind(id, NOW, NOW),
    DB.prepare("INSERT INTO steam_logins (id, account_id, created_at) VALUES ('login-1', ?, ?)").bind(id, NOW),
    DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES (?, '76561198000000001', ?)").bind(id, NOW),
    DB.prepare(
      "INSERT INTO client_modules" +
        " (account_id, state, rules_version, module_count, unknown_count, matched, worst_state," +
        "  worst_at, updated_at) VALUES (?, 'ok', 1, 40, 0, '[]', 'ok', ?, ?)",
    ).bind(id, NOW, NOW),
    DB.prepare(
      "INSERT INTO client_module_seen" +
        " (account_id, name, origin, state, sha256, first_at, last_at)" +
        " VALUES (?, 'd3d11.dll', 'system', 'ok', '', ?, ?)",
    ).bind(id, NOW, NOW),
    DB.prepare(
      "INSERT INTO client_crashes (account_id, rider_name, site, crashed_at, received_at)" +
        " VALUES (?, 'Frost', 'mxbikes.exe+0x11D753', ?, ?)",
    ).bind(id, NOW, NOW),
    DB.prepare(
      "INSERT INTO guid_claims (account_id, guid, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?)",
    ).bind(id, "FF011000013A7C2E91", NOW, NOW),
  ]);
}

async function count(DB: DB, table: string, id: string): Promise<number> {
  const row = await DB.prepare(`SELECT COUNT(*) AS n FROM ${table} WHERE account_id = ?`)
    .bind(id)
    .first<{ n: number }>();
  return Number(row?.n ?? 0);
}

const CALLER: Account = {
  id: "acc-1",
  rider_name: "Frost",
  steam_id: "76561198000000001",
  guid: "FF011000013A7C2E91",
};

describe("erasing an account", () => {
  it("empties every table that describes the person", async () => {
    const DB = d1();
    await account(DB, "acc-1");
    await fillPersonal(DB, "acc-1");

    const res = await eraseAccount(CALLER, { DB } as unknown as Env);
    expect(res.status).toBe(200);

    for (const table of [
      "loadouts",
      "loadout_paints",
      "presence",
      "server_queue",
      "steam_logins",
      "steam_links",
      "client_modules",
      "client_module_seen",
      "client_crashes",
      "guid_claims",
    ]) {
      expect(await count(DB, table, "acc-1"), table).toBe(0);
    }
  });

  it("leaves an account row that identifies nobody and can do nothing", async () => {
    const DB = d1();
    await account(DB, "acc-1");

    await eraseAccount(CALLER, { DB } as unknown as Env);

    const row = await DB.prepare(
      "SELECT rider_name, steam_id, guid, kind, token_hash, erased_at FROM accounts WHERE id = 'acc-1'",
    ).first<{
      rider_name: string;
      steam_id: string | null;
      guid: string | null;
      kind: string;
      token_hash: string;
      erased_at: number | null;
    }>();

    expect(row?.steam_id).toBeNull();
    expect(row?.guid).toBeNull();
    expect(row?.rider_name).not.toContain("Frost");
    // Not 'invited', so `invitedOnly` refuses it, and out of the rider-name unique index.
    expect(row?.kind).toBe("erased");
    // Not 64 hex characters, so no token can hash to it. The credential is gone, not rotated.
    expect(row?.token_hash).not.toMatch(/^[0-9a-f]{64}$/);
    expect(row?.erased_at).toBeGreaterThan(0);
  });

  it("does not take the token of a second erased account with it", async () => {
    const DB = d1();
    await account(DB, "acc-1");
    await account(DB, "acc-2", { rider_name: "Frost2", steam_id: "76561198000000002", guid: null });

    await eraseAccount(CALLER, { DB } as unknown as Env);
    // Both cleared rows have to coexist: `token_hash` is UNIQUE and the rider-name index is
    // unique over invited accounts. A constant in either column would fail here.
    const res = await eraseAccount(
      { id: "acc-2", rider_name: "Frost2", steam_id: "76561198000000002", guid: null },
      { DB } as unknown as Env,
    );
    expect(res.status).toBe(200);
  });

  it("keeps a ban and the claims that prove it, so this is not ban evasion", async () => {
    const DB = d1();
    await account(DB, "acc-1");
    await fillPersonal(DB, "acc-1");
    await DB.prepare(
      "INSERT INTO guid_bans (guid, reason, banned_at, banned_by) VALUES (?, 'cheating', ?, 'frost')",
    )
      .bind("FF011000013A7C2E91", NOW)
      .run();

    const res = await eraseAccount(CALLER, { DB } as unknown as Env);
    const body = (await res.json()) as { kept: { what: string }[] };

    expect(await count(DB, "guid_claims", "acc-1")).toBe(1);
    // By guid: the migrations seed bans of their own, so "the only row" is not this row.
    const ban = await DB.prepare("SELECT guid FROM guid_bans WHERE guid = ?")
      .bind("FF011000013A7C2E91")
      .first<{ guid: string }>();
    expect(ban?.guid).toBe("FF011000013A7C2E91");
    expect(body.kept.some((k) => k.what.includes("guid_bans"))).toBe(true);
    // Everything that is not the ban record still goes.
    expect(await count(DB, "client_module_seen", "acc-1")).toBe(0);
  });

  it("leaves purchases alone, and says that it did", async () => {
    const DB = d1();
    await account(DB, "acc-1");
    await DB.batch([
      DB.prepare("INSERT INTO assets (id, creator_id, title, created_at) VALUES ('ast-1', 'acc-1', 'A track', ?)").bind(NOW),
      DB.prepare(
        "INSERT INTO entitlements (steam_id, asset_id, source, granted_at)" +
          " VALUES ('76561198000000009', 'ast-1', 'purchase', ?)",
      ).bind(NOW),
    ]);

    const res = await eraseAccount(CALLER, { DB } as unknown as Env);
    const body = (await res.json()) as { kept: { what: string }[] };

    const left = await DB.prepare("SELECT COUNT(*) AS n FROM entitlements").first<{ n: number }>();
    expect(Number(left?.n)).toBe(1);
    expect(body.kept.some((k) => k.what.includes("entitlements"))).toBe(true);
  });

  it("survives the references that make a hard delete impossible", async () => {
    const DB = d1();
    await account(DB, "acc-1");
    // The three shapes that would fail a `DELETE FROM accounts` on a foreign key: an invite
    // it was claimed with, an asset it owns, a server it registered.
    await DB.batch([
      DB.prepare("INSERT INTO invites (code, created_at, claimed_by, claimed_at) VALUES ('CODE-1', ?, 'acc-1', ?)").bind(NOW, NOW),
      DB.prepare("INSERT INTO assets (id, creator_id, title, created_at) VALUES ('ast-1', 'acc-1', 'A track', ?)").bind(NOW),
      DB.prepare(
        "INSERT INTO server_roster (address, first_seen, last_seen, owner_account, owner_at)" +
          " VALUES ('203.0.113.7:54200', ?, ?, 'acc-1', ?)",
      ).bind(NOW, NOW, NOW),
    ]);

    const res = await eraseAccount(CALLER, { DB } as unknown as Env);
    expect(res.status).toBe(200);

    // The invite stays claimed — releasing it would hand a used code back out.
    const invite = await DB.prepare("SELECT claimed_by FROM invites WHERE code = 'CODE-1'").first<{
      claimed_by: string;
    }>();
    expect(invite?.claimed_by).toBe("acc-1");
    // The address stays in the shared book; whose it was does not.
    const server = await DB.prepare(
      "SELECT owner_account FROM server_roster WHERE address = '203.0.113.7:54200'",
    ).first<{ owner_account: string | null }>();
    expect(server?.owner_account).toBeNull();
  });

  it("answers the same way twice, because the app cannot retry with a dead token", async () => {
    const DB = d1();
    await account(DB, "acc-1");
    await fillPersonal(DB, "acc-1");

    const first = await eraseAccount(CALLER, { DB } as unknown as Env);
    const second = await eraseAccount(CALLER, { DB } as unknown as Env);

    expect(first.status).toBe(200);
    expect(second.status).toBe(200);
  });
});
