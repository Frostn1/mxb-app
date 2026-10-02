/**
 * View-only and locked paints: what an owner says about who may keep and who may wear a paint.
 *
 *  - **View-only.** Other riders on the server still see it, but their app installs it for the
 *    session only and deletes it when the game exits (`apps/manager/src-tauri/src/viewonly.rs`).
 *    Only apps that declare they understand the flag (`caps: ["viewOnly"]` on the join) are sent
 *    a view-only paint at all; older apps and the roster route never see one. This deters casual
 *    keeping. It cannot stop someone copying the file while the game has it open, or capturing
 *    the textures off the GPU: the game has to have the paint to draw it.
 *  - **Locked.** Only the owner and the team list may *wear* it. The team list is `paint_shares`
 *    rows for the paint (an account by rider name, or a GUID), the same rows that already let
 *    those riders download it. mxbserver servers with `[paints] enforce_locks` fetch the signed
 *    list below and send stock instead when anyone else joins wearing it. Locking needs a
 *    Steam-verified owner: a non-Steam GUID is first-come here, so it cannot anchor a lock.
 *
 * The policy rows are additive (`0051_paint_policies.sql`): a paint without one behaves exactly
 * as it always has.
 */

import { unb64url } from "./verdict";
import { guidFromSteamId } from "./steam";
import { isRelDest, isSha256 } from "./validate";

/** What the app says on a join when it can honour view-only paints. */
export const VIEW_ONLY_CAP = "viewOnly";

/** Team list cap per paint. A team, not a guest list. */
export const MAX_TEAM = 32;

/** The only GUID shape mxbserver can read off a connection, so the only one a lock can name. */
const WIRE_GUID = /^FF[0-9A-F]{16}$/;

export interface PolicyAccount {
  id: string;
  steam_id: string | null;
  guid: string | null;
}

export interface Result {
  status: number;
  body: unknown;
}

/** Whether a join's `caps` says the app can honour view-only paints. */
export function wantsViewOnly(caps: unknown): boolean {
  return Array.isArray(caps) && caps.includes(VIEW_ONLY_CAP);
}

/**
 * The account's GUID, when Valve vouches for it: a Steam sign-in whose derived GUID is the one
 * on the account. Anything else is first-come and cannot be the anchor of a lock.
 */
export function verifiedGuid(account: { steam_id: string | null; guid: string | null }): string | null {
  if (!account.steam_id || !account.guid) return null;
  const derived = guidFromSteamId(account.steam_id);
  return derived && derived === account.guid.toUpperCase() ? derived : null;
}

/**
 * The bike folder and paint name a game identity carries for this paint, folded, or null when
 * the paint is not a bike paint (gear paints are not in the identity's paint slot, so a server
 * cannot refuse them).
 *
 * MX Bikes picks a paint by the name its file was listed under, so this is what a server
 * matches on. Two owners locking different artwork under one name would both match it; the
 * file name is the only thing the wire carries.
 */
export function lockTarget(relDest: string, bikeId: string): { bike: string; paint: string } | null {
  const segs = relDest.trim().replace(/\\/g, "/").split("/");
  const file = segs[segs.length - 1] ?? "";
  if (!/\.pnt$/i.test(file)) return null;
  const paint = file.slice(0, -4).trim().toLowerCase();
  if (!paint) return null;
  if (segs.length === 4 && segs[0]!.toLowerCase() === "bikes" && segs[2]!.toLowerCase() === "paints") {
    return { bike: segs[1]!.toLowerCase(), paint };
  }
  if (segs.length >= 3 && segs[0]!.toLowerCase() === "paints" && segs[1]!.toLowerCase() === "bikes") {
    return { bike: bikeId.trim().toLowerCase(), paint };
  }
  return null;
}

interface TeamEntry {
  kind: "account" | "guid";
  /** The rider name for an account (as typed), the GUID for a GUID. */
  id: string;
}

interface PolicyBody {
  viewOnly: boolean;
  locked: boolean;
  team: TeamEntry[];
}

function parsePolicy(body: unknown): PolicyBody | string {
  if (!body || typeof body !== "object") return "expected a JSON body";
  const b = body as { viewOnly?: unknown; locked?: unknown; team?: unknown };
  if (typeof b.viewOnly !== "boolean" || typeof b.locked !== "boolean") {
    return "viewOnly and locked must both be true or false";
  }
  const rawTeam = b.team ?? [];
  if (!Array.isArray(rawTeam)) return "team must be a list";
  if (rawTeam.length > MAX_TEAM) return `a team list holds at most ${MAX_TEAM} riders`;
  const team: TeamEntry[] = [];
  for (const raw of rawTeam) {
    const e = (raw ?? {}) as { kind?: unknown; id?: unknown };
    if (typeof e.id !== "string" || !e.id.trim() || e.id.length > 64) return "each team entry needs an id";
    if (e.kind === "guid") {
      const guid = e.id.trim().toUpperCase();
      if (!WIRE_GUID.test(guid)) return `${e.id.trim()} isn't an MX Bikes GUID (FF and 16 hex digits)`;
      team.push({ kind: "guid", id: guid });
    } else if (e.kind === "account") {
      team.push({ kind: "account", id: e.id.trim() });
    } else {
      return "a team entry is an account or a guid";
    }
  }
  return { viewOnly: b.viewOnly, locked: b.locked, team };
}

/**
 * `GET /v1/paints/policies` — the caller's own paints, each with its policy and team list.
 *
 * Every paint in their current look, plus any paint they set a policy on and no longer wear,
 * so a lock can always be lifted.
 */
export async function listPolicies(account: PolicyAccount, env: Env): Promise<Result> {
  const worn = await env.DB.prepare(
    "SELECT sha256, MIN(rel_dest) AS rel_dest, MIN(bike_id) AS bike_id, MIN(slot) AS slot, MIN(file_name) AS file_name" +
      " FROM loadout_paints WHERE account_id = ? GROUP BY sha256",
  )
    .bind(account.id)
    .all<{ sha256: string; rel_dest: string; bike_id: string; slot: string; file_name: string }>();
  const policies = await env.DB.prepare(
    "SELECT sha256, rel_dest, bike_id, view_only, locked FROM paint_policies WHERE owner_account_id = ?",
  )
    .bind(account.id)
    .all<{ sha256: string; rel_dest: string; bike_id: string; view_only: number; locked: number }>();
  const shares = await env.DB.prepare(
    "SELECT s.sha256, s.grantee_kind, s.grantee, a.rider_name FROM paint_shares s" +
      " LEFT JOIN accounts a ON s.grantee_kind = 'account' AND a.id = s.grantee" +
      " WHERE s.owner_account_id = ? AND s.grantee_kind IN ('account', 'guid')" +
      " ORDER BY s.created_at",
  )
    .bind(account.id)
    .all<{ sha256: string; grantee_kind: string; grantee: string; rider_name: string | null }>();

  const team = new Map<string, TeamEntry[]>();
  for (const s of shares.results ?? []) {
    const list = team.get(s.sha256) ?? [];
    // An account that has since been erased has no name to show; it is dropped from the list
    // the next time the owner saves.
    if (s.grantee_kind === "account" && !s.rider_name) continue;
    list.push({ kind: s.grantee_kind as TeamEntry["kind"], id: s.grantee_kind === "account" ? s.rider_name! : s.grantee });
    team.set(s.sha256, list);
  }

  const out = new Map<string, Record<string, unknown>>();
  for (const w of worn.results ?? []) {
    out.set(w.sha256, {
      sha256: w.sha256,
      fileName: w.file_name,
      relDest: w.rel_dest,
      bikeId: w.bike_id,
      slot: w.slot,
      worn: true,
      viewOnly: false,
      locked: false,
      lockable: lockTarget(w.rel_dest, w.bike_id) !== null,
      team: team.get(w.sha256) ?? [],
    });
  }
  for (const p of policies.results ?? []) {
    const row = out.get(p.sha256) ?? {
      sha256: p.sha256,
      fileName: p.rel_dest.split("/").pop() ?? p.rel_dest,
      relDest: p.rel_dest,
      bikeId: p.bike_id,
      slot: null,
      worn: false,
      lockable: lockTarget(p.rel_dest, p.bike_id) !== null,
      team: team.get(p.sha256) ?? [],
    };
    row.viewOnly = !!p.view_only;
    row.locked = !!p.locked;
    out.set(p.sha256, row);
  }
  return {
    status: 200,
    body: { canLock: verifiedGuid(account) !== null, paints: [...out.values()] },
  };
}

/**
 * `PUT /v1/paints/:sha/policy` — set View-only, Locked and the team list for one of the
 * caller's own paints.
 *
 * Only a paint in the caller's own look (or one they already hold a policy on). Saving all
 * three off with an empty team removes the policy, so the paint is back to how it always was.
 * The team list replaces the paint's `paint_shares` rows.
 */
export async function putPolicy(request: Request, sha256: string, account: PolicyAccount, env: Env): Promise<Result> {
  if (!isSha256(sha256)) return { status: 400, body: { error: "that isn't a paint hash" } };
  let raw: unknown;
  try {
    raw = await request.json();
  } catch {
    return { status: 400, body: { error: "expected a JSON body" } };
  }
  const policy = parsePolicy(raw);
  if (typeof policy === "string") return { status: 400, body: { error: policy } };

  const worn = await env.DB.prepare(
    "SELECT rel_dest, bike_id FROM loadout_paints WHERE account_id = ? AND sha256 = ? ORDER BY slot = 'paint' DESC LIMIT 1",
  )
    .bind(account.id, sha256)
    .first<{ rel_dest: string; bike_id: string }>();
  const held = await env.DB.prepare("SELECT rel_dest, bike_id FROM paint_policies WHERE owner_account_id = ? AND sha256 = ?")
    .bind(account.id, sha256)
    .first<{ rel_dest: string; bike_id: string }>();
  const where = worn ?? held;
  if (!where || !isRelDest(where.rel_dest)) {
    return { status: 404, body: { error: "that paint isn't in your look" } };
  }
  if (policy.locked) {
    if (!verifiedGuid(account)) {
      return {
        status: 409,
        body: {
          error: "Locking a paint needs Steam sign-in: without it your GUID is only claimed, not proven, so a lock could not tell you apart from someone using it.",
          code: "steam_required",
        },
      };
    }
    if (!lockTarget(where.rel_dest, where.bike_id)) {
      return {
        status: 400,
        body: { error: "Only bike paints can be locked: a server sees which bike paint a rider wears, and nothing about their gear." },
      };
    }
  }

  // Team accounts are named by rider name, which is what a rider knows about another.
  const grants: { kind: "account" | "guid"; grantee: string }[] = [];
  for (const e of policy.team) {
    if (e.kind === "guid") {
      grants.push({ kind: "guid", grantee: e.id });
      continue;
    }
    const found = await env.DB.prepare("SELECT id FROM accounts WHERE lower(rider_name) = lower(?) AND erased_at IS NULL LIMIT 2")
      .bind(e.id)
      .all<{ id: string }>();
    const ids = found.results ?? [];
    if (ids.length !== 1) {
      return { status: 400, body: { error: `no single MXB App rider is called ${e.id}; add their GUID instead` } };
    }
    if (ids[0]!.id !== account.id) grants.push({ kind: "account", grantee: ids[0]!.id });
  }

  const now = Date.now();
  const empty = !policy.viewOnly && !policy.locked && grants.length === 0;
  const statements = [
    env.DB.prepare("DELETE FROM paint_shares WHERE owner_account_id = ? AND sha256 = ? AND grantee_kind IN ('account', 'guid')").bind(
      account.id,
      sha256,
    ),
    empty
      ? env.DB.prepare("DELETE FROM paint_policies WHERE owner_account_id = ? AND sha256 = ?").bind(account.id, sha256)
      : env.DB.prepare(
          "INSERT INTO paint_policies (owner_account_id, sha256, rel_dest, bike_id, view_only, locked, updated_at)" +
            " VALUES (?, ?, ?, ?, ?, ?, ?)" +
            " ON CONFLICT (owner_account_id, sha256) DO UPDATE SET rel_dest = excluded.rel_dest, bike_id = excluded.bike_id," +
            " view_only = excluded.view_only, locked = excluded.locked, updated_at = excluded.updated_at",
        ).bind(account.id, sha256, where.rel_dest, where.bike_id ?? "", policy.viewOnly ? 1 : 0, policy.locked ? 1 : 0, now),
    ...grants.map((g) =>
      env.DB.prepare(
        "INSERT OR REPLACE INTO paint_shares (sha256, owner_account_id, grantee_kind, grantee, created_at, expires_at) VALUES (?, ?, ?, ?, ?, NULL)",
      ).bind(sha256, account.id, g.kind, g.grantee, now),
    ),
  ];
  await env.DB.batch(statements);
  return { status: 200, body: { ok: true, sha256, viewOnly: policy.viewOnly, locked: policy.locked, team: policy.team } };
}

// ── The server's lock list ──────────────────────────────────────────────────────────────────

/** One locked paint, as mxbserver matches it. Field order is the wire format. */
export interface LockEntry {
  sha256: string;
  /** Bike folder, folded. */
  bike: string;
  /** Paint name as the game identity carries it (file name without `.pnt`), folded. */
  paint: string;
  /** GUIDs that may wear it, uppercase. The owner first. */
  allowed: string[];
}

/** Every lock in force, with who may wear each. */
export async function lockList(env: Env): Promise<LockEntry[]> {
  const rows = await env.DB.prepare(
    "SELECT pp.sha256, pp.rel_dest, pp.bike_id, pp.owner_account_id, a.steam_id, a.guid" +
      " FROM paint_policies pp JOIN accounts a ON a.id = pp.owner_account_id" +
      " WHERE pp.locked = 1 AND a.erased_at IS NULL ORDER BY pp.sha256, pp.owner_account_id",
  ).all<{ sha256: string; rel_dest: string; bike_id: string; owner_account_id: string; steam_id: string | null; guid: string | null }>();
  const out: LockEntry[] = [];
  for (const r of rows.results ?? []) {
    const owner = verifiedGuid(r);
    const target = lockTarget(r.rel_dest, r.bike_id);
    // An owner who is no longer Steam-verified has no proven GUID: the lock is not served
    // rather than served with nobody allowed, which would take their own paint off them.
    if (!owner || !target) continue;
    const team = await env.DB.prepare(
      "SELECT s.grantee_kind, s.grantee, a.steam_id, a.guid FROM paint_shares s" +
        " LEFT JOIN accounts a ON s.grantee_kind = 'account' AND a.id = s.grantee" +
        " WHERE s.owner_account_id = ? AND s.sha256 = ? AND (s.expires_at IS NULL OR s.expires_at > ?)",
    )
      .bind(r.owner_account_id, r.sha256, Date.now())
      .all<{ grantee_kind: string; grantee: string; steam_id: string | null; guid: string | null }>();
    const allowed = [owner];
    for (const t of team.results ?? []) {
      // A team account counts by its proven GUID only; a GUID typed by the owner counts as typed.
      const guid = t.grantee_kind === "guid" ? t.grantee.toUpperCase() : t.grantee_kind === "account" ? verifiedGuid(t) : null;
      if (guid && WIRE_GUID.test(guid) && !allowed.includes(guid)) allowed.push(guid);
    }
    out.push({ sha256: r.sha256, bike: target.bike, paint: target.paint, allowed });
  }
  return out;
}

/** The trusted comment on every lock list signature. */
export const LOCKS_TRUSTED_COMMENT = "mxb paint locks v1";

interface LockKey {
  key: CryptoKey;
  /** minisign key id: the first 8 bytes of SHA-256 of the public key. */
  keyId: Uint8Array;
  publicKey: Uint8Array;
}

/** `MXB_PAINTLOCK_SIGNING_KEY`: an Ed25519 PKCS#8 key, base64 or base64url. Null when unset. */
export async function lockKey(env: Env): Promise<LockKey | null> {
  const raw = env.MXB_PAINTLOCK_SIGNING_KEY?.replace(/\s+/g, "");
  if (!raw) return null;
  try {
    const key = await crypto.subtle.importKey("pkcs8", unb64url(raw), { name: "Ed25519" }, true, ["sign"]);
    const jwk = (await crypto.subtle.exportKey("jwk", key)) as JsonWebKey;
    const publicKey = unb64url(jwk.x ?? "");
    if (publicKey.length !== 32) return null;
    const keyId = new Uint8Array(await crypto.subtle.digest("SHA-256", publicKey)).slice(0, 8);
    return { key, keyId, publicKey };
  } catch (err) {
    console.error(JSON.stringify({ msg: "paint lock signing key unusable", error: String(err) }));
    return null;
  }
}

function b64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** The public key in minisign's `.pub` format, which is what mxbserver's `[paints] pubkey_file` holds. */
export function minisignPublicKey(k: LockKey): string {
  return `untrusted comment: mxb paint lock public key\n${b64(concat(new Uint8Array([0x45, 0x64]), k.keyId, k.publicKey))}\n`;
}

/**
 * Sign `message` as a minisign signature (legacy `Ed`, not prehashed), which mxbserver checks
 * with the `minisign-verify` crate it already uses for releases.
 */
export async function minisignSign(message: Uint8Array, k: LockKey, trusted = LOCKS_TRUSTED_COMMENT): Promise<string> {
  const sig = new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, k.key, message));
  const global = new Uint8Array(
    await crypto.subtle.sign({ name: "Ed25519" }, k.key, concat(sig, new TextEncoder().encode(trusted))),
  );
  return [
    "untrusted comment: mxb control plane paint locks",
    b64(concat(new Uint8Array([0x45, 0x64]), k.keyId, sig)),
    `trusted comment: ${trusted}`,
    b64(global),
    "",
  ].join("\n");
}

/**
 * `GET /v1/servers/paint-locks` — the signed lock list, for a managed server.
 *
 * Authenticated by the server's rating token (the one it already pushes results with). The
 * body is `{ payload, minisig }`: `payload` is the JSON string signed, so the server verifies
 * the exact bytes it then parses, and can keep both on disk and re-verify them at its next start.
 */
export async function signedLocks(serverId: string, env: Env, now = Date.now()): Promise<Result> {
  const k = await lockKey(env);
  if (!k) return { status: 503, body: { error: "paint lock signing is not configured" } };
  const payload = JSON.stringify({ v: 1, server: serverId, issuedAt: now, locks: await lockList(env) });
  return { status: 200, body: { payload, minisig: await minisignSign(new TextEncoder().encode(payload), k) } };
}

