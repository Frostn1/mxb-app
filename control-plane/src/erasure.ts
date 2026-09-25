/**
 * "Delete everything you have about me", answered by the app itself.
 *
 * A right of erasure that runs through a mailbox is a right in name: it takes days, it needs
 * somebody at a keyboard, and the person asking has to trust a stranger's word that it
 * happened. This is the same request as one endpoint, authenticated by the token the app
 * already holds, answered in one round trip with the list of what went.
 *
 * ## It is a clearing, not a `DELETE FROM accounts`
 *
 * Tables point at an account id from every direction and most of those references have no
 * cascade: the invite it was claimed with, the assets a creator owns, the servers they
 * registered. Dropping the row would fail on a foreign key for almost every invited account,
 * and where it succeeded it would take a creator's catalogue with it. So the row survives as
 * an opaque key with nothing personal left on it (`0040_account_erasure.sql` says how), and
 * every satellite row that describes a person is deleted outright.
 *
 * ## Two things are deliberately kept
 *
 * **A ban, and the GUID claims that prove it is yours.** Otherwise this endpoint is the
 * cheapest ban evasion in the product: ask to be forgotten, re-enroll, ride. Erasure is not
 * absolute in the regulation either — a controller may keep what it needs for overriding
 * legitimate interests, and "this person was removed for cheating" is the example the
 * argument is made with. So a banned identity keeps its `guid_claims` rows and its
 * `guid_bans` row. Everything else on it still goes, and the response says so rather than
 * quietly doing less than it claims.
 *
 * That includes the device links (`device_links`, `devices.ts`): the keyed hash of the machine
 * the account was used on is deleted for every account, banned or not. It describes a person's
 * hardware rather than what they did, and the ban already survives through the GUID claims, so it
 * is not one of the things kept.
 *
 * **Purchases.** `entitlements` is keyed by Steam id, not by account, and it is what makes a
 * locked track open on the machine of the person who paid for it. Deleting it would take
 * content off a buyer and a name off a creator's buyer list, which is not what anybody means
 * by this request. It stays, and it is the one thing named in the answer that was not erased.
 *
 * ## What it cannot reach, because it was never linked
 *
 * Usage counters and survey answers are keyed by a random install id that is tied to no
 * account on purpose, so nothing here can find the rows belonging to one person — which is
 * the same property that makes them safe to hold. The switch for those is in the app, under
 * Settings, and it is the only control that was ever going to work on them.
 */

import type { Ban } from "./bans";
import { banFor } from "./bans";

/** The caller, as the route table already resolved it. */
export interface Account {
  id: string;
  rider_name: string;
  steam_id?: string | null;
  guid?: string | null;
}

/** What one erasure did, in the order the answer reads. */
export interface Erasure {
  /** Tables that had rows for this account and now have none. */
  cleared: string[];
  /** Kept, with the reason, because a silent exception is the same as not doing it. */
  kept: { what: string; why: string }[];
  erasedAt: number;
}

/**
 * The tables whose rows exist only to describe a person, in the order they are deleted.
 *
 * All of them are keyed on `account_id`. Order is not a constraint — nothing here points at
 * anything else here — but it reads as the shape of what is held: what you published, where
 * you were, how you signed in, which machine you used, what we looked at inside your game, and
 * where it crashed.
 */
const PERSONAL: readonly string[] = [
  "loadout_paints",
  "loadouts",
  "presence",
  "server_queue",
  "steam_logins",
  "steam_links",
  "device_links",
  "client_modules",
  "client_module_seen",
  "client_crashes",
];

/**
 * A token hash that cannot be the hash of a token.
 *
 * `hashToken` returns 64 hex characters, so a value with a colon in it can never be produced
 * by hashing anything. The column is `NOT NULL UNIQUE` and has to hold something; this is the
 * something that makes the credential unpresentable rather than merely rotated.
 */
function deadTokenHash(): string {
  return `erased:${crypto.randomUUID()}`;
}

/**
 * Erase the calling account.
 *
 * Idempotent: a second call on an already-erased account finds nothing to delete, writes the
 * same cleared row again and answers the same way. That matters because the app cannot retry
 * this — its token stopped working the moment the first one succeeded — so a client that
 * lost the response has no way to tell a success from a failure except by asking again with
 * a token that is now dead. Better that the second attempt reads as done than as an error.
 */
export async function eraseAccount(account: Account, env: Env): Promise<Response> {
  const now = Date.now();

  // Asked before anything is deleted: this is the only moment the claim log is still here to
  // resolve a ban through. Afterwards the question cannot be answered.
  const ban: Ban | null = await banFor(env, {
    accountId: account.id,
    steamId: account.steam_id ?? null,
    guid: account.guid ?? null,
  });

  const statements = PERSONAL.map((table) =>
    env.DB.prepare(`DELETE FROM ${table} WHERE account_id = ?`).bind(account.id),
  );

  // A registered server is somebody else's joinable address, not a fact about its owner. The
  // ownership link is the personal part, so that is what goes.
  statements.push(
    env.DB.prepare(
      "UPDATE server_roster SET owner_account = NULL, owner_at = NULL WHERE owner_account = ?",
    ).bind(account.id),
  );

  if (!ban) {
    statements.push(
      env.DB.prepare("DELETE FROM guid_claims WHERE account_id = ?").bind(account.id),
    );
  }

  // Last, so a failure anywhere above leaves an account that still works rather than one
  // nobody can use and whose data is half gone. D1 batches are one transaction, which makes
  // this ordering belt and braces — and it is the order to keep if that ever stops being true.
  statements.push(
    env.DB.prepare(
      "UPDATE accounts SET rider_name = ?, steam_id = NULL, guid = NULL, kind = 'erased'," +
        " token_hash = ?, erased_at = ? WHERE id = ?",
    ).bind(`erased-${account.id.slice(0, 8)}`, deadTokenHash(), now, account.id),
  );

  await env.DB.batch(statements);

  const kept: Erasure["kept"] = [];
  if (ban) {
    kept.push({
      what: "guid_bans, guid_claims",
      why: "an account removed for cheating keeps the record that proves the removal is its own",
    });
  }
  kept.push({
    what: "entitlements",
    why: "purchases are keyed by Steam id and are what keep bought content working",
  });

  const erasure: Erasure = { cleared: [...PERSONAL, "accounts (cleared in place)"], kept, erasedAt: now };
  console.log(JSON.stringify({ msg: "account erased", account: account.id, banned: Boolean(ban) }));
  return json(200, erasure);
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8" },
  });
}
