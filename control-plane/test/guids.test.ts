/**
 * The GUID is derived from the Valve-confirmed Steam identity, never trusted from the client.
 *
 * These are the anti-spoof guarantees, against the real router and a real SQLite: a Steam
 * player's GUID is computed from the identity Valve vouched for and pinned, a spoofer holding
 * it is dispossessed, and the client's own claim is ignored for a Steam account.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { pinGuidFromSteam } from "../src/steamlink";
import { guidFromSteamId } from "../src/steam";
import { hashToken } from "../src/auth";
import { addAccount, d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const OWNER = "acc_owner";
/** Two Steam accounts, and the GUIDs they derive to. */
const ALICE = "76561197984950104";
const BOB = "76561198174305985";
const ALICE_GUID = guidFromSteamId(ALICE)!;
const BOB_GUID = guidFromSteamId(BOB)!;

async function env(): Promise<Env> {
  const DB = d1();
  await addAccount(DB, OWNER, "Owner");
  return { DB } as unknown as Env;
}

/** An app account with a bearer token, a Steam id and a starting GUID. */
async function account(
  db: Env["DB"],
  id: string,
  token: string,
  steamId: string | null,
  guid: string | null,
) {
  await db
    .prepare("INSERT INTO accounts (id, rider_name, steam_id, guid, token_hash, created_at) VALUES (?, ?, ?, ?, ?, ?)")
    .bind(id, id, steamId, guid, await hashToken(token), Date.now())
    .run();
}

const guidOf = (db: Env["DB"], id: string) =>
  db.prepare("SELECT guid FROM accounts WHERE id = ?").bind(id).first<{ guid: string | null }>();

const call = (e: Env, r: Request) => worker.fetch(r, e, {} as ExecutionContext);
const putGuid = (token: string, guid: string) =>
  new Request("https://cp.test/v1/me/guid", {
    method: "PUT",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify({ guid }),
  });

describe("pinning a GUID from the Steam identity", () => {
  it("derives and sets it, and records the claim", async () => {
    const e = await env();
    await account(e.DB, "acc_a", "a", ALICE, null);
    expect(await pinGuidFromSteam(e, "acc_a", ALICE)).toBe(ALICE_GUID);
    expect(await guidOf(e.DB, "acc_a")).toEqual({ guid: ALICE_GUID });
    expect(
      await e.DB.prepare("SELECT guid FROM guid_claims WHERE account_id = 'acc_a'").first(),
    ).toEqual({ guid: ALICE_GUID });
  });

  it("takes the GUID back from whoever was holding it — Valve's word beats first-come", async () => {
    const e = await env();
    // A spoofer claimed Alice's GUID before she linked.
    await account(e.DB, "acc_spoof", "s", null, ALICE_GUID);
    await account(e.DB, "acc_alice", "a", ALICE, null);

    expect(await pinGuidFromSteam(e, "acc_alice", ALICE)).toBe(ALICE_GUID);
    expect(await guidOf(e.DB, "acc_alice")).toEqual({ guid: ALICE_GUID });
    // The impostor is dispossessed rather than left colliding on the unique index.
    expect(await guidOf(e.DB, "acc_spoof")).toEqual({ guid: null });
  });

  it("leaves a non-Steam (Piboso) identity alone — nothing to derive", async () => {
    const e = await env();
    await account(e.DB, "acc_piboso", "p", null, "ABCDEF0123456789AB");
    // A made-up id below the SteamID64 base is not derivable.
    expect(await pinGuidFromSteam(e, "acc_piboso", "123")).toBeNull();
    expect(await guidOf(e.DB, "acc_piboso")).toEqual({ guid: "ABCDEF0123456789AB" });
  });
});

describe("PUT /v1/me/guid", () => {
  it("ignores the client's value for a Steam account and stores the derived one", async () => {
    const e = await env();
    await account(e.DB, "acc_alice", "alice-token", ALICE, null);

    // Alice's app tries to claim Bob's GUID — a spoof, or a stale value. It doesn't matter which.
    const res = await call(e, putGuid("alice-token", BOB_GUID));
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ ok: true, guid: ALICE_GUID });
    expect(await guidOf(e.DB, "acc_alice")).toEqual({ guid: ALICE_GUID });
  });

  it("reclaims through the router when a Steam account claims its own held-elsewhere GUID", async () => {
    const e = await env();
    await account(e.DB, "acc_spoof", "s", null, ALICE_GUID);
    await account(e.DB, "acc_alice", "alice-token", ALICE, null);

    expect((await call(e, putGuid("alice-token", ALICE_GUID))).status).toBe(200);
    expect(await guidOf(e.DB, "acc_alice")).toEqual({ guid: ALICE_GUID });
    expect(await guidOf(e.DB, "acc_spoof")).toEqual({ guid: null });
  });

  it("is first-come for an unlinked (Piboso) account", async () => {
    const e = await env();
    await account(e.DB, "acc_piboso", "piboso-token", null, null);
    const guid = "ABCDEF0123456789AB";
    expect((await call(e, putGuid("piboso-token", guid))).status).toBe(200);
    expect(await guidOf(e.DB, "acc_piboso")).toEqual({ guid });

    // And another account cannot take one already held.
    await account(e.DB, "acc_other", "other-token", null, null);
    const clash = await call(e, putGuid("other-token", guid));
    expect(clash.status).toBe(409);
  });
});

describe("the backfill migration (0039)", () => {
  // Re-run the migration's statements after seeding, to prove the SQL derivation and de-spoof.
  const sql = readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), "..", "migrations", "0039_derive_guids.sql"),
    "utf8",
  );
  const run = async (db: Env["DB"]) => {
    // Strip comment lines first, then split into statements — a chunk that merely begins with a
    // comment still carries a real statement after it.
    const clean = sql.split("\n").filter((l) => !l.trim().startsWith("--")).join("\n");
    for (const stmt of clean.split(";").map((x) => x.trim()).filter(Boolean)) {
      await db.prepare(stmt).run();
    }
  };

  it("derives every Steam account's GUID and clears the spoof holding it", async () => {
    const e = await env();
    // A spoofer holding Alice's derived GUID, Alice linked but with a wrong stored GUID, Bob
    // linked with none, and a Piboso account with its own opaque GUID.
    await account(e.DB, "acc_spoof", "s", null, ALICE_GUID);
    await account(e.DB, "acc_alice", "a", ALICE, "FF000000000000DEAD");
    await account(e.DB, "acc_bob", "b", BOB, null);
    await account(e.DB, "acc_piboso", "p", null, "ABCDEF0123456789AB");

    await run(e.DB);

    expect(await guidOf(e.DB, "acc_alice")).toEqual({ guid: ALICE_GUID });
    expect(await guidOf(e.DB, "acc_bob")).toEqual({ guid: BOB_GUID });
    expect(await guidOf(e.DB, "acc_spoof")).toEqual({ guid: null });
    // The non-Steam account is untouched.
    expect(await guidOf(e.DB, "acc_piboso")).toEqual({ guid: "ABCDEF0123456789AB" });
    // The derived GUIDs are recorded as claims.
    const claims = await e.DB
      .prepare("SELECT account_id, guid FROM guid_claims WHERE account_id IN ('acc_alice','acc_bob') ORDER BY account_id")
      .all<{ account_id: string; guid: string }>();
    expect(claims.results).toEqual([
      { account_id: "acc_alice", guid: ALICE_GUID },
      { account_id: "acc_bob", guid: BOB_GUID },
    ]);
  });
});
