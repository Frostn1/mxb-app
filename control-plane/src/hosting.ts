/**
 * User server deploy: invited Steam accounts get a game server of their own, in the region
 * and of the type they pick, in a slot on an OVH VPS the control plane orders when it needs
 * one. servers.mxbsecure.com drives it; MSM drives one server with a claimed token.
 *
 * Money is the limit, not boxes. A box is ordered only when a deploy finds no free slot in its
 * pool and region, and only if `MXB_HOST_BOX_PRICE_USD` x (billed boxes + 1) stays within
 * `MXB_HOST_SPEND_CAP_USD`. VPSes bill monthly, so an empty box is never cancelled here: it is
 * drained, then flagged near its renewal date for an operator to cancel at OVH by hand.
 *
 * Users never hold a box or slot credential. Every call to a box goes through `slotCall`, which
 * builds the URL from the box's own recorded IP and a fixed path, so a stored row can never
 * aim a request anywhere else.
 */

import { bearer, hashToken, newToken, tokenMatches } from "./auth";
import {
  BIKE_SETS,
  BOX_OS,
  MAX_RIDERS,
  REGIONS,
  SERVER_TYPES,
  poolFor,
  regionById,
  slotPorts,
  type HostRegion,
  type Pool,
  type ServerType,
} from "./hostregions";
import { OvhClient, ovhCredentials } from "./ovh";
import { isSteamId64 } from "./steam";

export interface Result {
  status: number;
  body: unknown;
}

export interface Deps {
  fetch: typeof fetch;
  now: () => number;
  /** The OVH client, or null when the deployment has no OVH credentials. */
  ovh: OvhClient | null;
}

export function defaultDeps(env: Env, fetchImpl: typeof fetch = fetch): Deps {
  const creds = ovhCredentials(env);
  return { fetch: fetchImpl, now: () => Date.now(), ovh: creds ? new OvhClient(creds, fetchImpl) : null };
}

const DAY = 24 * 60 * 60 * 1000;
const MINUTE = 60 * 1000;

/** States a box is billed in (and counted against the cap in). */
const BILLED = ["ordering", "delivering", "rebuilding", "installing", "ready", "draining", "flagged"];
/** States of a box that is on its way but has no slots yet. */
const PENDING = ["ordering", "delivering", "rebuilding", "installing"];

export const PROGRESS_STEPS = ["Provisioning", "Installing", "Ready"];
export const NEW_BOX_NOTE = "A new server can take from a few minutes up to a day.";
export const INSTALL_NOTE = "Installing the server. This usually takes a few minutes.";

// ---- Config --------------------------------------------------------------------------------

export interface HostConfig {
  capUsd: number;
  boxPriceUsd: number;
  maxBoxes: number;
  legacyMaxBoxes: number;
  slots: Record<Pool, number>;
  idleDays: number;
  emptyDays: number;
  flagBeforeRenewalDays: number;
  preprovision: boolean;
  subsidiary: string;
  installRepo: string;
}

function num(value: string | undefined, fallback: number): number {
  const n = Number(value);
  return value !== undefined && value.trim() !== "" && Number.isFinite(n) && n >= 0 ? n : fallback;
}

export function hostConfig(env: Env): HostConfig {
  return {
    capUsd: num(env.MXB_HOST_SPEND_CAP_USD, 0),
    boxPriceUsd: num(env.MXB_HOST_BOX_PRICE_USD, 5.85),
    maxBoxes: Math.floor(num(env.MXB_HOST_MAX_BOXES, 4)),
    legacyMaxBoxes: Math.floor(num(env.MXB_HOST_LEGACY_MAX_BOXES, 1)),
    slots: {
      native: Math.max(1, Math.floor(num(env.MXB_HOST_SLOTS_NATIVE, 4))),
      legacy: Math.max(1, Math.floor(num(env.MXB_HOST_SLOTS_LEGACY, 2))),
    },
    idleDays: num(env.MXB_HOST_IDLE_DAYS, 7),
    emptyDays: num(env.MXB_HOST_EMPTY_DAYS, 7),
    flagBeforeRenewalDays: 3,
    preprovision: String(env.MXB_HOST_PREPROVISION) === "1",
    subsidiary: env.MXB_HOST_OVH_SUBSIDIARY?.trim() || "US",
    installRepo: env.MXB_HOST_INSTALL_REPO?.trim() || "Frostn1/mxbserver-releases",
  };
}

// ---- Rows ----------------------------------------------------------------------------------

export interface BoxRow {
  id: string;
  pool: Pool;
  region: string;
  datacenter: string;
  plan_code: string;
  state: string;
  /** Column name predates the USD switch (migration 0057); the value is USD. */
  price_eur: number;
  quoted_price: number | null;
  quoted_currency: string | null;
  ovh_order_id: number | null;
  ovh_service: string | null;
  ip: string | null;
  slots_total: number;
  agent_token: string | null;
  renews_at: number | null;
  stage_at: number;
  empty_since: number | null;
  flagged_at: number | null;
  cancelled_at: number | null;
  last_error: string | null;
  install_log: string | null;
  created_at: number;
}

export interface SlotRow {
  id: string;
  box_id: string;
  idx: number;
  game_port: number;
  token: string | null;
  server_id: string | null;
}

export interface ServerRow {
  id: string;
  steam_id: string;
  name: string;
  type: ServerType;
  region: string;
  box_id: string | null;
  slot_id: string | null;
  state: "waiting" | "ready" | "failed" | "deleted";
  track: string | null;
  bike_set: string | null;
  max_riders: number;
  applied: number;
  riders: number | null;
  polled_at: number | null;
  last_active_at: number;
  error: string | null;
  created_at: number;
  ready_at: number | null;
  deleted_at: number | null;
}

async function box(env: Env, id: string | null): Promise<BoxRow | null> {
  if (!id) return null;
  return env.DB.prepare("SELECT * FROM host_boxes WHERE id = ?").bind(id).first<BoxRow>();
}

async function slot(env: Env, id: string | null): Promise<SlotRow | null> {
  if (!id) return null;
  return env.DB.prepare("SELECT * FROM host_slots WHERE id = ?").bind(id).first<SlotRow>();
}

async function serverRow(env: Env, id: string): Promise<ServerRow | null> {
  return env.DB.prepare("SELECT * FROM host_servers WHERE id = ? AND state != 'deleted'").bind(id).first<ServerRow>();
}

function inList(states: string[]): string {
  return states.map((s) => `'${s}'`).join(", ");
}

// ---- Alerts --------------------------------------------------------------------------------

export async function alert(
  env: Env,
  deps: Deps,
  kind: string,
  message: string,
  extra: { region?: string | null; boxId?: string | null } = {},
): Promise<void> {
  const now = deps.now();
  // One open alert per kind and subject: a refusal that repeats on every deploy is one line.
  const open = await env.DB.prepare(
    "SELECT id FROM host_alerts WHERE kind = ? AND COALESCE(region, '') = ? AND COALESCE(box_id, '') = ? AND acked_at IS NULL",
  )
    .bind(kind, extra.region ?? "", extra.boxId ?? "")
    .first<{ id: string }>();
  if (open) {
    await env.DB.prepare("UPDATE host_alerts SET message = ?, created_at = ? WHERE id = ?").bind(message, now, open.id).run();
    return;
  }
  await env.DB.prepare(
    "INSERT INTO host_alerts (id, kind, region, box_id, message, created_at) VALUES (?, ?, ?, ?, ?, ?)",
  )
    .bind(crypto.randomUUID(), kind, extra.region ?? null, extra.boxId ?? null, message, now)
    .run();
  console.log(JSON.stringify({ msg: "hosting alert", kind, region: extra.region ?? null, box: extra.boxId ?? null }));
  const hook = env.MXB_HOST_ALERT_WEBHOOK_URL?.trim();
  if (hook) {
    try {
      await deps.fetch(hook, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ content: `Server hosting: ${message}` }),
      });
    } catch {
      // The alert is in D1 either way; the webhook is a nudge.
    }
  }
}

// ---- Spend ---------------------------------------------------------------------------------

export interface Spend {
  capUsd: number;
  committedUsd: number;
  boxPriceUsd: number;
  maxBoxes: number;
  boxes: number;
  legacyBoxes: number;
}

/** What is billed now: boxes in a billed state, plus failed ones OVH delivered (still billed). */
export async function spend(env: Env, cfg: HostConfig): Promise<Spend> {
  const rows = await env.DB.prepare(
    `SELECT pool, price_eur FROM host_boxes
      WHERE state IN (${inList(BILLED)}) OR (state = 'failed' AND ovh_service IS NOT NULL)`,
  ).all<{ pool: Pool; price_eur: number }>();
  const committed = rows.results.reduce((sum, r) => sum + r.price_eur, 0);
  return {
    capUsd: cfg.capUsd,
    committedUsd: Math.round(committed * 100) / 100,
    boxPriceUsd: cfg.boxPriceUsd,
    maxBoxes: cfg.maxBoxes,
    boxes: rows.results.length,
    legacyBoxes: rows.results.filter((r) => r.pool === "legacy").length,
  };
}

/** Whether one more box fits, and if not, why (for the operator alert). */
export function roomForBox(s: Spend, cfg: HostConfig, pool: Pool): string | null {
  if (s.committedUsd + cfg.boxPriceUsd > cfg.capUsd + 1e-9) {
    return `the monthly spend cap (USD ${cfg.capUsd.toFixed(2)}) would be passed: USD ${s.committedUsd.toFixed(2)} committed, a box is USD ${cfg.boxPriceUsd.toFixed(2)}`;
  }
  if (s.boxes + 1 > cfg.maxBoxes) return `the box limit (${cfg.maxBoxes}) is reached`;
  if (pool === "legacy" && s.legacyBoxes + 1 > cfg.legacyMaxBoxes) {
    return `the Legacy box limit (${cfg.legacyMaxBoxes}) is reached`;
  }
  return null;
}

// ---- Placement -----------------------------------------------------------------------------

/**
 * The free slot to put a server in: on the fullest placeable box in the pool and region that
 * still has one (best fit), so tenants pack and emptier boxes can drain.
 */
export async function bestFitSlot(env: Env, pool: Pool, region: string, states = ["ready"]): Promise<SlotRow | null> {
  return env.DB.prepare(
    `SELECT s.* FROM host_slots s JOIN host_boxes b ON b.id = s.box_id
      WHERE b.pool = ? AND b.region = ? AND b.state IN (${inList(states)}) AND s.server_id IS NULL
      ORDER BY (SELECT COUNT(*) FROM host_slots u WHERE u.box_id = b.id AND u.server_id IS NOT NULL) DESC,
               b.created_at, s.idx
      LIMIT 1`,
  )
    .bind(pool, region)
    .first<SlotRow>();
}

/** A box still on its way in this pool and region with room for one more waiting server. */
async function pendingBoxWithRoom(env: Env, pool: Pool, region: string): Promise<BoxRow | null> {
  return env.DB.prepare(
    `SELECT b.* FROM host_boxes b
      WHERE b.pool = ? AND b.region = ? AND b.state IN (${inList(PENDING)})
        AND b.slots_total > (SELECT COUNT(*) FROM host_servers v WHERE v.box_id = b.id AND v.state = 'waiting')
      ORDER BY b.created_at LIMIT 1`,
  )
    .bind(pool, region)
    .first<BoxRow>();
}

/** Order one box. Returns it, or the reason it was not ordered (already alerted). */
export async function orderBox(
  env: Env,
  deps: Deps,
  cfg: HostConfig,
  pool: Pool,
  region: HostRegion,
): Promise<BoxRow | string> {
  const s = await spend(env, cfg);
  const refused = roomForBox(s, cfg, pool);
  if (refused) {
    await alert(env, deps, "capacity", `No box ordered in ${region.label} (${pool}): ${refused}.`, { region: region.id });
    return refused;
  }
  if (!deps.ovh) {
    await alert(env, deps, "capacity", `No box ordered in ${region.label}: the OVH credentials are not set.`, { region: region.id });
    return "no OVH credentials";
  }
  const now = deps.now();
  const id = crypto.randomUUID();
  // The row goes in before the order, so a box can never be billed with nothing pointing at it.
  await env.DB.prepare(
    `INSERT INTO host_boxes (id, pool, region, datacenter, plan_code, state, price_eur, slots_total, stage_at, created_at)
     VALUES (?, ?, ?, ?, ?, 'ordering', ?, ?, ?, ?)`,
  )
    .bind(id, pool, region.id, region.datacenter, region.planCode, cfg.boxPriceUsd, cfg.slots[pool], now, now)
    .run();
  try {
    const order = await deps.ovh.orderVps({
      subsidiary: cfg.subsidiary,
      planCode: region.planCode,
      datacenter: region.datacenter,
      os: BOX_OS,
      addons: [region.osAddon, region.storageAddon, region.backupAddon],
    });
    await env.DB.prepare(
      "UPDATE host_boxes SET state = 'delivering', ovh_order_id = ?, quoted_price = ?, quoted_currency = ?, stage_at = ? WHERE id = ?",
    )
      .bind(order.orderId, order.price, order.currency, deps.now(), id)
      .run();
    console.log(JSON.stringify({ msg: "hosting box ordered", box: id, region: region.id, pool, order: order.orderId }));
  } catch (err) {
    await env.DB.prepare("UPDATE host_boxes SET state = 'failed', last_error = ?, stage_at = ? WHERE id = ?")
      .bind(String(err).slice(0, 500), deps.now(), id)
      .run();
    await alert(env, deps, "order_failed", `Ordering a box in ${region.label} failed: ${String(err).slice(0, 300)}`, {
      region: region.id,
      boxId: id,
    });
    return "the order failed";
  }
  return (await box(env, id))!;
}

// ---- Users and invites ---------------------------------------------------------------------

export async function hostUser(env: Env, steamId: string): Promise<{ quota: number; suspended: number } | null> {
  return env.DB.prepare("SELECT quota, suspended FROM host_users WHERE steam_id = ?").bind(steamId).first();
}

function codeText(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(18));
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export async function mintInvite(
  env: Env,
  deps: Deps,
  operator: string,
  input: Record<string, unknown>,
): Promise<Result> {
  const steamId = typeof input.steamId === "string" && input.steamId.trim() ? input.steamId.trim() : null;
  if (steamId && !isSteamId64(steamId)) return { status: 400, body: { error: "that isn't a SteamID64" } };
  const quota = input.quota === undefined ? 1 : Number(input.quota);
  if (!Number.isInteger(quota) || quota < 1 || quota > 5) return { status: 400, body: { error: "quota must be 1 to 5" } };
  const days = input.expiresDays === undefined ? 14 : Number(input.expiresDays);
  if (!Number.isInteger(days) || days < 1 || days > 90) return { status: 400, body: { error: "expiry must be 1 to 90 days" } };
  const code = codeText();
  const id = crypto.randomUUID();
  const now = deps.now();
  await env.DB.prepare(
    "INSERT INTO host_invites (id, code_hash, created_by, steam_id, quota, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
  )
    .bind(id, await hashToken(code), operator, steamId, quota, now + days * DAY, now)
    .run();
  return { status: 201, body: { id, code, link: `https://servers.mxbsecure.com/?invite=${code}` } };
}

export async function claimInvite(env: Env, deps: Deps, steamId: string, input: Record<string, unknown>): Promise<Result> {
  const code = typeof input.code === "string" ? input.code.trim() : "";
  if (!/^[A-Za-z0-9_-]{16,64}$/.test(code)) return { status: 400, body: { error: "That invite isn't valid." } };
  const now = deps.now();
  const invite = await env.DB.prepare("SELECT * FROM host_invites WHERE code_hash = ?")
    .bind(await hashToken(code))
    .first<{ id: string; steam_id: string | null; quota: number; expires_at: number; claimed_by: string | null; revoked_at: number | null }>();
  if (!invite || invite.revoked_at || invite.expires_at < now) {
    return { status: 404, body: { error: "That invite isn't valid or has expired." } };
  }
  if (invite.claimed_by && invite.claimed_by !== steamId) return { status: 409, body: { error: "That invite was already used." } };
  if (invite.steam_id && invite.steam_id !== steamId) return { status: 403, body: { error: "That invite is for another Steam account." } };
  if (!invite.claimed_by) {
    await env.DB.prepare("UPDATE host_invites SET claimed_by = ?, claimed_at = ? WHERE id = ? AND claimed_by IS NULL")
      .bind(steamId, now, invite.id)
      .run();
    await env.DB.prepare(
      `INSERT INTO host_users (steam_id, quota, invite_id, created_at) VALUES (?, ?, ?, ?)
       ON CONFLICT(steam_id) DO UPDATE SET quota = MAX(host_users.quota, excluded.quota)`,
    )
      .bind(steamId, invite.quota, invite.id, now)
      .run();
  }
  const user = await hostUser(env, steamId);
  return { status: 200, body: { ok: true, quota: user?.quota ?? invite.quota } };
}

// ---- The server view -----------------------------------------------------------------------

export async function tracksFor(env: Env, pool: Pool): Promise<{ id: string; name: string }[]> {
  const rows = await env.DB.prepare("SELECT id, name FROM host_tracks WHERE pool = ? ORDER BY name COLLATE NOCASE")
    .bind(pool)
    .all<{ id: string; name: string }>();
  return rows.results;
}

export async function serverView(env: Env, cfg: HostConfig, row: ServerRow): Promise<Record<string, unknown>> {
  const b = await box(env, row.box_id);
  const s = await slot(env, row.slot_id);
  const region = regionById(row.region);
  const pool = poolFor(row.type);
  let state: string;
  let step: number;
  let note: string | null = null;
  let since = row.created_at;
  if (row.state === "ready") {
    state = "ready";
    step = 3;
    since = row.ready_at ?? row.created_at;
  } else if (row.state === "failed") {
    state = "failed";
    step = 0;
  } else if (!b || PENDING.slice(0, 3).includes(b.state)) {
    state = "provisioning";
    step = 1;
    note = NEW_BOX_NOTE;
    since = b?.created_at ?? row.created_at;
  } else {
    state = "installing";
    step = 2;
    note = INSTALL_NOTE;
    since = b.stage_at;
  }
  return {
    id: row.id,
    name: row.name,
    type: row.type,
    region: row.region,
    regionLabel: region?.label ?? row.region,
    state,
    progress: { step, steps: PROGRESS_STEPS, since, note },
    address: row.state === "ready" && b?.ip && s ? `${b.ip}:${s.game_port}` : null,
    settings: { track: row.track, bikeSet: row.type === "mxbserver" ? row.bike_set : null, maxRiders: row.max_riders },
    options: {
      tracks: await tracksFor(env, pool),
      bikeSets: row.type === "mxbserver" ? BIKE_SETS : [],
      maxRiders: MAX_RIDERS,
    },
    riders: row.riders,
    idleSince: row.state === "ready" && !row.riders ? row.last_active_at : null,
    freedAt: row.state === "ready" ? row.last_active_at + cfg.idleDays * DAY : null,
    createdAt: row.created_at,
    error: row.error,
  };
}

export async function myHosting(env: Env, steamId: string, operator: boolean): Promise<Result> {
  const cfg = hostConfig(env);
  const user = await hostUser(env, steamId);
  const rows = await env.DB.prepare(
    "SELECT * FROM host_servers WHERE steam_id = ? AND state != 'deleted' ORDER BY created_at",
  )
    .bind(steamId)
    .all<ServerRow>();
  const servers = [];
  for (const row of rows.results) servers.push(await serverView(env, cfg, row));
  return {
    status: 200,
    body: {
      operator,
      invited: Boolean(user && !user.suspended),
      quota: user?.quota ?? 0,
      servers,
      regions: REGIONS.map((r) => ({ id: r.id, label: r.label })),
      types: SERVER_TYPES,
    },
  };
}

// ---- Deploy --------------------------------------------------------------------------------

function cleanName(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const name = value.trim().replace(/\s+/g, " ");
  if (name.length < 1 || name.length > 40 || /[\u0000-\u001f\u007f"\\[\]]/.test(name)) return null;
  return name;
}

export async function deploy(env: Env, deps: Deps, steamId: string, input: Record<string, unknown>): Promise<Result> {
  const cfg = hostConfig(env);
  const user = await hostUser(env, steamId);
  if (!user || user.suspended) return { status: 403, body: { code: "not_invited", error: "Servers are invite-only." } };
  const name = cleanName(input.name);
  if (!name) return { status: 400, body: { error: "Give the server a name of up to 40 characters." } };
  const type = SERVER_TYPES.find((t) => t.id === input.type)?.id;
  if (!type) return { status: 400, body: { error: "Pick a server type." } };
  const region = regionById(input.region);
  if (!region) return { status: 400, body: { error: "Pick a region." } };

  const active = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM host_servers WHERE steam_id = ? AND state IN ('waiting', 'ready')",
  )
    .bind(steamId)
    .first<{ n: number }>();
  if ((active?.n ?? 0) >= user.quota) {
    return { status: 403, body: { code: "quota", error: `You can run ${user.quota} server${user.quota === 1 ? "" : "s"} at a time.` } };
  }

  const pool = poolFor(type);
  const now = deps.now();
  const id = crypto.randomUUID();
  const tracks = await tracksFor(env, pool);
  await env.DB.prepare(
    `INSERT INTO host_servers (id, steam_id, name, type, region, state, track, bike_set, max_riders, last_active_at, created_at)
     VALUES (?, ?, ?, ?, ?, 'waiting', ?, ?, ?, ?, ?)`,
  )
    .bind(id, steamId, name, type, region.id, tracks[0]?.id ?? null, type === "mxbserver" ? BIKE_SETS[0].id : null, MAX_RIDERS, now, now)
    .run();

  const placed = await place(env, deps, cfg, id, pool, region);
  if (placed !== "ok") {
    await env.DB.prepare("DELETE FROM host_servers WHERE id = ?").bind(id).run();
    return { status: 409, body: { code: "no_capacity", error: `No capacity in ${region.label} right now.` } };
  }
  console.log(JSON.stringify({ msg: "hosting deploy", server: id, steamId, type, region: region.id }));
  if (cfg.preprovision) await maybePreprovision(env, deps, cfg, pool, region);
  return { status: 201, body: { server: await serverView(env, cfg, (await serverRow(env, id))!) } };
}

/**
 * Find the server a home: a free slot on a ready box; else on a draining or flagged box
 * (cheaper than buying one, and it brings that box back); else a box already on its way;
 * else a new box, inside the cap.
 */
async function place(env: Env, deps: Deps, cfg: HostConfig, serverId: string, pool: Pool, region: HostRegion): Promise<"ok" | string> {
  let free = await bestFitSlot(env, pool, region.id);
  if (!free) {
    free = await bestFitSlot(env, pool, region.id, ["draining", "flagged"]);
    if (free) {
      await env.DB.prepare("UPDATE host_boxes SET state = 'ready', flagged_at = NULL, stage_at = ? WHERE id = ?")
        .bind(deps.now(), free.box_id)
        .run();
      await env.DB.prepare("UPDATE host_alerts SET acked_at = ? WHERE box_id = ? AND kind = 'flagged' AND acked_at IS NULL")
        .bind(deps.now(), free.box_id)
        .run();
    }
  }
  if (free) {
    await assign(env, deps, serverId, free);
    return "ok";
  }
  let target = await pendingBoxWithRoom(env, pool, region.id);
  if (!target) {
    const ordered = await orderBox(env, deps, cfg, pool, region);
    if (typeof ordered === "string") return ordered;
    target = ordered;
  }
  await env.DB.prepare("UPDATE host_servers SET box_id = ? WHERE id = ?").bind(target.id, serverId).run();
  return "ok";
}

async function assign(env: Env, deps: Deps, serverId: string, free: SlotRow): Promise<void> {
  const claimed = await env.DB.prepare("UPDATE host_slots SET server_id = ? WHERE id = ? AND server_id IS NULL")
    .bind(serverId, free.id)
    .run();
  if (!claimed.meta.changes) throw new Error("slot was taken");
  await env.DB.prepare("UPDATE host_servers SET box_id = ?, slot_id = ?, state = 'ready', ready_at = ?, last_active_at = ? WHERE id = ?")
    .bind(free.box_id, free.id, deps.now(), deps.now(), serverId)
    .run();
  await env.DB.prepare("UPDATE host_boxes SET empty_since = NULL WHERE id = ?").bind(free.box_id).run();
  const row = await serverRow(env, serverId);
  if (row) await applySettings(env, deps, row);
}

async function maybePreprovision(env: Env, deps: Deps, cfg: HostConfig, pool: Pool, region: HostRegion): Promise<void> {
  if (await bestFitSlot(env, pool, region.id)) return;
  if (await pendingBoxWithRoom(env, pool, region.id)) return;
  const recent = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM host_servers WHERE region = ? AND type IN (" +
      (pool === "legacy" ? "'legacy'" : "'mxbserver'") +
      ") AND created_at > ?",
  )
    .bind(region.id, deps.now() - 7 * DAY)
    .first<{ n: number }>();
  if ((recent?.n ?? 0) < 2) return;
  const s = await spend(env, cfg);
  if (roomForBox(s, cfg, pool)) return; // quietly: pre-provisioning is a nicety
  await orderBox(env, deps, cfg, pool, region);
}

// ---- Talking to a slot ---------------------------------------------------------------------

/** The HTTPS base Caddy serves a slot under on its box. Built from our own row, never input. */
export function slotBase(b: BoxRow, s: SlotRow): string | null {
  if (!b.ip || !/^\d{1,3}(\.\d{1,3}){3}$/.test(b.ip)) return null;
  const host = `${b.ip.replace(/\./g, "-")}.sslip.io`;
  return b.pool === "native" ? `https://${host}/s${s.idx}` : `https://${host}/agent/instances/s${s.idx}`;
}

async function slotCall(
  deps: Deps,
  b: BoxRow,
  s: SlotRow,
  path: string,
  init: { method: string; body?: unknown },
): Promise<{ ok: boolean; status: number; body: unknown }> {
  const base = slotBase(b, s);
  const token = b.pool === "native" ? s.token : b.agent_token;
  if (!base || !token) return { ok: false, status: 502, body: null };
  try {
    const res = await deps.fetch(`${base}${path}`, {
      method: init.method,
      redirect: "manual",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: init.body === undefined ? undefined : JSON.stringify(init.body),
      signal: AbortSignal.timeout(10_000),
    });
    const text = await res.text();
    let body: unknown = null;
    try {
      body = text ? JSON.parse(text.slice(0, 256 * 1024)) : null;
    } catch {
      body = null;
    }
    return { ok: res.ok, status: res.status, body };
  } catch {
    return { ok: false, status: 502, body: null };
  }
}

/** The whole server.toml for a native slot. Ports, listen address and admin are pinned here. */
export function nativeConfig(input: {
  name: string;
  index: number;
  maxRiders: number;
  track: string | null;
  bikeSet: string | null;
}): string {
  const { gamePort, adminPort } = slotPorts(input.index);
  const q = (s: string) => JSON.stringify(s);
  const lines = [
    "# Written by the MXB control plane for a hosted slot. Changes here are overwritten.",
    "[server]",
    `name = ${q(input.name)}`,
    `listen = ${q(`0.0.0.0:${gamePort}`)}`,
    `max_clients = ${Math.min(MAX_RIDERS, Math.max(1, input.maxRiders))}`,
    "",
    "[track]",
    `package = ${q(`/etc/mxbserver/tracks/${input.track ?? "default"}.pkz`)}`,
    "",
    "[bike_set]",
    `manifest = ${q(`/etc/mxbserver/bike-sets/${input.bikeSet ?? BIKE_SETS[0].id}.toml`)}`,
    "",
    "[admin]",
    `listen = ${q(`127.0.0.1:${adminPort}`)}`,
    `tokens_file = ${q(`/etc/mxbserver/s${input.index}/admin-tokens.toml`)}`,
    "",
  ];
  return lines.join("\n");
}

/** Push a server's settings to its slot. Marks it applied on success. */
export async function applySettings(env: Env, deps: Deps, row: ServerRow): Promise<boolean> {
  const b = await box(env, row.box_id);
  const s = await slot(env, row.slot_id);
  if (!b || !s) return false;
  let result;
  if (b.pool === "native") {
    if (!row.track) return false; // no track installed yet: the slot keeps the box default
    result = await slotCall(deps, b, s, "/v1/config/write", {
      method: "POST",
      body: {
        content: nativeConfig({ name: row.name, index: s.idx, maxRiders: row.max_riders, track: row.track, bikeSet: row.bike_set }),
        drain_seconds: 0,
      },
    });
  } else {
    result = await slotCall(deps, b, s, "/config", {
      method: "PUT",
      body: { name: row.name, maxClients: row.max_riders, ...(row.track ? { track: row.track } : {}) },
    });
  }
  await env.DB.prepare("UPDATE host_servers SET applied = ? WHERE id = ?").bind(result.ok ? 1 : 0, row.id).run();
  if (!result.ok) console.error(JSON.stringify({ msg: "hosting apply failed", server: row.id, status: result.status }));
  return result.ok;
}

/** Put a slot back to an unowned default and free it. */
async function resetSlot(env: Env, deps: Deps, s: SlotRow): Promise<void> {
  const b = await box(env, s.box_id);
  if (b) {
    if (b.pool === "native") {
      const tracks = await tracksFor(env, "native");
      if (tracks[0]) {
        await slotCall(deps, b, s, "/v1/config/write", {
          method: "POST",
          body: {
            content: nativeConfig({ name: `Free slot ${s.idx}`, index: s.idx, maxRiders: MAX_RIDERS, track: tracks[0].id, bikeSet: null }),
            drain_seconds: 0,
          },
        });
      }
    } else {
      await slotCall(deps, b, s, "/stop", { method: "POST" });
    }
  }
  await env.DB.prepare("UPDATE host_slots SET server_id = NULL WHERE id = ?").bind(s.id).run();
}

// ---- Owner actions -------------------------------------------------------------------------

export async function getServer(env: Env, steamId: string, id: string): Promise<Result> {
  const row = await serverRow(env, id);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  return { status: 200, body: { server: await serverView(env, hostConfig(env), row) } };
}

export async function updateSettings(
  env: Env,
  deps: Deps,
  steamId: string,
  id: string,
  input: Record<string, unknown>,
): Promise<Result> {
  const row = await serverRow(env, id);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  const pool = poolFor(row.type);
  let track = row.track;
  let bikeSet = row.bike_set;
  let maxRiders = row.max_riders;
  if (input.track !== undefined) {
    const tracks = await tracksFor(env, pool);
    if (typeof input.track !== "string" || !tracks.some((t) => t.id === input.track)) {
      return { status: 400, body: { error: "That track isn't available." } };
    }
    track = input.track;
  }
  if (input.bikeSet !== undefined) {
    if (row.type !== "mxbserver" || !BIKE_SETS.some((b) => b.id === input.bikeSet)) {
      return { status: 400, body: { error: "That bike set isn't available." } };
    }
    bikeSet = input.bikeSet as string;
  }
  if (input.maxRiders !== undefined) {
    const n = Number(input.maxRiders);
    if (!Number.isInteger(n) || n < 1 || n > MAX_RIDERS) {
      return { status: 400, body: { error: `The rider cap must be 1 to ${MAX_RIDERS}.` } };
    }
    maxRiders = n;
  }
  await env.DB.prepare("UPDATE host_servers SET track = ?, bike_set = ?, max_riders = ?, applied = 0 WHERE id = ?")
    .bind(track, bikeSet, maxRiders, id)
    .run();
  const updated = (await serverRow(env, id))!;
  if (updated.state === "ready" && !(await applySettings(env, deps, updated))) {
    return { status: 502, body: { error: "Saved, but the server didn't take the change yet. It will be retried." } };
  }
  return { status: 200, body: { server: await serverView(env, hostConfig(env), (await serverRow(env, id))!) } };
}

export async function restartServer(env: Env, deps: Deps, steamId: string, id: string): Promise<Result> {
  const row = await serverRow(env, id);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  const b = await box(env, row.box_id);
  const s = await slot(env, row.slot_id);
  if (row.state !== "ready" || !b || !s) return { status: 409, body: { error: "The server isn't ready yet." } };
  const result =
    b.pool === "native"
      ? await slotCall(deps, b, s, "/v1/restart", { method: "POST", body: { drain_seconds: 0, reason: "owner restart" } })
      : await slotCall(deps, b, s, "/restart", { method: "POST" });
  return result.ok ? { status: 200, body: { ok: true } } : { status: 502, body: { error: "The server didn't restart." } };
}

export async function deleteServer(env: Env, deps: Deps, id: string, why: string): Promise<Result> {
  const row = await serverRow(env, id);
  if (!row) return { status: 404, body: { error: "No such server." } };
  const s = await slot(env, row.slot_id);
  if (s && s.server_id === row.id) await resetSlot(env, deps, s);
  const now = deps.now();
  await env.DB.prepare("UPDATE host_servers SET state = 'deleted', deleted_at = ?, slot_id = NULL WHERE id = ?").bind(now, id).run();
  await env.DB.prepare("UPDATE host_tokens SET revoked_at = ? WHERE server_id = ? AND revoked_at IS NULL").bind(now, id).run();
  console.log(JSON.stringify({ msg: "hosting delete", server: id, why }));
  return { status: 200, body: { ok: true } };
}

export async function ownerDelete(env: Env, deps: Deps, steamId: string, id: string): Promise<Result> {
  const row = await serverRow(env, id);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  return deleteServer(env, deps, id, "owner");
}

// ---- MSM -----------------------------------------------------------------------------------

const CLAIM_TTL_MS = 10 * MINUTE;

export async function msmLink(env: Env, deps: Deps, steamId: string, id: string): Promise<Result> {
  const row = await serverRow(env, id);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  const code = codeText() + codeText();
  const expiresAt = deps.now() + CLAIM_TTL_MS;
  await env.DB.prepare("INSERT INTO host_claims (code_hash, server_id, steam_id, expires_at) VALUES (?, ?, ?, ?)")
    .bind(await hashToken(code), id, steamId, expiresAt)
    .run();
  return { status: 200, body: { url: `mxbservers://connect?claim=${code}`, expiresAt } };
}

export async function msmClaim(env: Env, deps: Deps, input: Record<string, unknown>): Promise<Result> {
  const code = typeof input.claim === "string" ? input.claim : "";
  if (!/^[A-Za-z0-9_-]{16,128}$/.test(code)) return { status: 400, body: { error: "That link isn't valid." } };
  const now = deps.now();
  const hash = await hashToken(code);
  const used = await env.DB.prepare(
    "UPDATE host_claims SET used_at = ? WHERE code_hash = ? AND used_at IS NULL AND expires_at > ? RETURNING server_id, steam_id",
  )
    .bind(now, hash, now)
    .first<{ server_id: string; steam_id: string }>();
  if (!used) return { status: 404, body: { error: "That link has expired or was already used. Open it again from the site." } };
  const row = await serverRow(env, used.server_id);
  if (!row || row.steam_id !== used.steam_id) return { status: 404, body: { error: "That server is gone." } };
  const token = newToken();
  await env.DB.prepare("INSERT INTO host_tokens (token_hash, server_id, steam_id, created_at) VALUES (?, ?, ?, ?)")
    .bind(await hashToken(token), row.id, row.steam_id, now)
    .run();
  return { status: 200, body: { token, server: await serverView(env, hostConfig(env), row) } };
}

/** The owner a hosted (MSM) bearer speaks for, if it is still good for this server. */
export async function hostedOwner(env: Env, request: Request, serverId: string): Promise<string | null> {
  const presented = bearer(request.headers.get("Authorization"));
  if (!presented) return null;
  const row = await env.DB.prepare(
    "SELECT steam_id FROM host_tokens WHERE token_hash = ? AND server_id = ? AND revoked_at IS NULL",
  )
    .bind(await hashToken(presented), serverId)
    .first<{ steam_id: string }>();
  return row?.steam_id ?? null;
}

// ---- The box install runner ----------------------------------------------------------------

export function runnerAuthorized(env: Env, request: Request): boolean {
  const key = env.MXB_BOX_ENROLL_KEY;
  const presented = bearer(request.headers.get("Authorization"));
  return Boolean(key && key.length >= 32 && presented && tokenMatches(key, presented));
}

export async function boxStage(env: Env, deps: Deps, id: string, input: Record<string, unknown>): Promise<Result> {
  const b = await box(env, id);
  if (!b) return { status: 404, body: { error: "no such box" } };
  const stage = typeof input.stage === "string" ? input.stage : "";
  const now = deps.now();
  if (stage === "installing") {
    if (!["rebuilding", "installing", "failed"].includes(b.state)) return { status: 409, body: { error: `box is ${b.state}` } };
    await env.DB.prepare("UPDATE host_boxes SET state = 'installing', stage_at = ?, last_error = NULL WHERE id = ?").bind(now, id).run();
    return { status: 200, body: { ok: true } };
  }
  if (stage === "failed") {
    const log = typeof input.log === "string" ? input.log.slice(-8000) : null;
    await env.DB.prepare("UPDATE host_boxes SET state = 'failed', stage_at = ?, last_error = 'install failed', install_log = ? WHERE id = ?")
      .bind(now, log, id)
      .run();
    await failWaiting(env, id);
    await alert(env, deps, "install_failed", `Installing box ${b.ovh_service ?? id} (${b.region}) failed. It is still billed; retry or cancel it.`, {
      region: b.region,
      boxId: id,
    });
    return { status: 200, body: { ok: true } };
  }
  return { status: 400, body: { error: "stage must be installing or failed" } };
}

async function failWaiting(env: Env, boxId: string): Promise<void> {
  await env.DB.prepare(
    "UPDATE host_servers SET state = 'failed', error = 'The server could not be set up. Delete it and try again.' WHERE box_id = ? AND state = 'waiting'",
  )
    .bind(boxId)
    .run();
}

export async function enrollBox(env: Env, deps: Deps, id: string, input: Record<string, unknown>): Promise<Result> {
  const b = await box(env, id);
  if (!b) return { status: 404, body: { error: "no such box" } };
  if (!["installing", "rebuilding", "ready", "failed"].includes(b.state)) return { status: 409, body: { error: `box is ${b.state}` } };
  const slots = Array.isArray(input.slots) ? (input.slots as Record<string, unknown>[]) : null;
  if (!slots || slots.length !== b.slots_total) return { status: 400, body: { error: `expected ${b.slots_total} slots` } };
  const seen = new Set<number>();
  for (const s of slots) {
    const index = Number(s.index);
    if (!Number.isInteger(index) || index < 1 || index > b.slots_total || seen.has(index)) return { status: 400, body: { error: "bad slot index" } };
    seen.add(index);
    if (Number(s.gamePort) !== slotPorts(index).gamePort) return { status: 400, body: { error: `slot ${index} has the wrong game port` } };
    if (b.pool === "native" && (typeof s.token !== "string" || s.token.length < 32 || s.token.length > 512)) {
      return { status: 400, body: { error: `slot ${index} needs its control token` } };
    }
  }
  const agentToken = typeof input.agentToken === "string" ? input.agentToken : null;
  if (b.pool === "legacy" && (!agentToken || agentToken.length < 32 || agentToken.length > 512)) {
    return { status: 400, body: { error: "a legacy box needs its agent token" } };
  }
  for (const s of slots) {
    const index = Number(s.index);
    await env.DB.prepare(
      `INSERT INTO host_slots (id, box_id, idx, game_port, token) VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(box_id, idx) DO UPDATE SET token = excluded.token, game_port = excluded.game_port`,
    )
      .bind(crypto.randomUUID(), id, index, slotPorts(index).gamePort, b.pool === "native" ? (s.token as string) : null)
      .run();
  }
  const now = deps.now();
  await env.DB.prepare(
    "UPDATE host_boxes SET state = 'ready', agent_token = ?, stage_at = ?, empty_since = ?, last_error = NULL WHERE id = ?",
  )
    .bind(b.pool === "legacy" ? agentToken : null, now, now, id)
    .run();
  console.log(JSON.stringify({ msg: "hosting box ready", box: id, region: b.region, pool: b.pool }));
  await seatWaiting(env, deps, id);
  return { status: 200, body: { ok: true } };
}

/** Move servers waiting on a box into its free slots. */
async function seatWaiting(env: Env, deps: Deps, boxId: string): Promise<void> {
  const waiting = await env.DB.prepare(
    "SELECT * FROM host_servers WHERE box_id = ? AND state = 'waiting' ORDER BY created_at",
  )
    .bind(boxId)
    .all<ServerRow>();
  for (const row of waiting.results) {
    const free = await env.DB.prepare("SELECT * FROM host_slots WHERE box_id = ? AND server_id IS NULL ORDER BY idx LIMIT 1")
      .bind(boxId)
      .first<SlotRow>();
    if (!free) break;
    await assign(env, deps, row.id, free);
  }
}

export async function runnerTracks(env: Env, url: URL): Promise<Result> {
  const pool = url.searchParams.get("pool");
  if (pool !== "native" && pool !== "legacy") return { status: 400, body: { error: "pool must be native or legacy" } };
  const rows = await env.DB.prepare("SELECT id, name, url, sha256 FROM host_tracks WHERE pool = ? ORDER BY created_at")
    .bind(pool)
    .all();
  return { status: 200, body: { tracks: rows.results } };
}

// ---- The cron tick -------------------------------------------------------------------------

async function dispatchInstall(env: Env, deps: Deps, cfg: HostConfig, b: BoxRow): Promise<boolean> {
  const token = env.MXB_GH_DISPATCH_TOKEN?.trim();
  if (!token || !b.ip) {
    await alert(env, deps, "install_blocked", `Box ${b.ovh_service} is up but MXB_GH_DISPATCH_TOKEN is not set, so it can't be installed.`, {
      boxId: b.id,
    });
    return false;
  }
  const res = await deps.fetch(
    `https://api.github.com/repos/${cfg.installRepo}/actions/workflows/box-install.yml/dispatches`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: "application/vnd.github+json",
        "User-Agent": "mxb-control-plane",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        ref: "main",
        inputs: {
          box_id: b.id,
          ip: b.ip,
          pool: b.pool,
          slots: String(b.slots_total),
          game_url: b.pool === "legacy" ? env.MXB_GAME_DOWNLOAD_URL ?? "" : "",
        },
      }),
    },
  );
  if (!res.ok) {
    await alert(env, deps, "install_blocked", `Starting the install for box ${b.ovh_service} failed (GitHub ${res.status}).`, { boxId: b.id });
    return false;
  }
  return true;
}

async function advanceBox(env: Env, deps: Deps, cfg: HostConfig, b: BoxRow): Promise<void> {
  const ovh = deps.ovh;
  const now = deps.now();
  if (!ovh) return;
  if (b.state === "delivering" && b.ovh_order_id) {
    const service = b.ovh_service ?? (await ovh.deliveredService(b.ovh_order_id));
    if (!service) {
      if (now - b.stage_at > 2 * DAY) {
        await alert(env, deps, "slow_delivery", `OVH order ${b.ovh_order_id} (${b.region}) is not delivered after 2 days.`, { boxId: b.id });
      }
      return;
    }
    const ip = await ovh.ipv4(service);
    const renews = await ovh.renewsAt(service);
    await env.DB.prepare("UPDATE host_boxes SET ovh_service = ?, ip = ?, renews_at = ? WHERE id = ?")
      .bind(service, ip, renews, b.id)
      .run();
    if (!ip) return;
    const key = env.MXB_HOST_SSH_PUBLIC_KEY?.trim();
    if (!key) {
      await alert(env, deps, "install_blocked", `Box ${service} is delivered but MXB_HOST_SSH_PUBLIC_KEY is not set.`, { boxId: b.id });
      return;
    }
    await ovh.rebuild(service, BOX_OS, key);
    await env.DB.prepare("UPDATE host_boxes SET state = 'rebuilding', stage_at = ? WHERE id = ?").bind(now, b.id).run();
    return;
  }
  if (b.state === "rebuilding" && b.ovh_service) {
    // OVH reports a rebuild as VPS tasks; give it a couple of minutes to register one.
    if (now - b.stage_at < 2 * MINUTE) return;
    if (await ovh.busy(b.ovh_service)) return;
    if ((await ovh.vps(b.ovh_service)).state !== "running") return;
    if (await dispatchInstall(env, deps, cfg, b)) {
      await env.DB.prepare("UPDATE host_boxes SET state = 'installing', stage_at = ? WHERE id = ?").bind(now, b.id).run();
    }
    return;
  }
  if (b.state === "installing" && now - b.stage_at > 3 * 60 * MINUTE) {
    await boxStage(env, deps, b.id, { stage: "failed", log: "no enrollment within 3 hours" });
  }
}

/** Ask each ready server how many riders it has, at most every 30 minutes. */
async function pollRiders(env: Env, deps: Deps): Promise<void> {
  const now = deps.now();
  const due = await env.DB.prepare(
    "SELECT * FROM host_servers WHERE state = 'ready' AND (polled_at IS NULL OR polled_at < ?) ORDER BY polled_at LIMIT 20",
  )
    .bind(now - 30 * MINUTE)
    .all<ServerRow>();
  for (const row of due.results) {
    const b = await box(env, row.box_id);
    const s = await slot(env, row.slot_id);
    if (!b || !s) continue;
    const res = await slotCall(deps, b, s, b.pool === "native" ? "/v1/riders" : "/players", { method: "GET" });
    let riders: number | null = null;
    if (res.ok) {
      const body = res.body as { riders?: unknown; players?: unknown } | unknown[] | null;
      if (Array.isArray(body)) riders = body.length;
      else if (body && Array.isArray((body as { riders?: unknown }).riders)) riders = ((body as { riders: unknown[] }).riders).length;
      else if (body && Array.isArray((body as { players?: unknown }).players)) riders = ((body as { players: unknown[] }).players).length;
    }
    await env.DB.prepare(
      "UPDATE host_servers SET riders = ?, polled_at = ?, last_active_at = CASE WHEN ? > 0 THEN ? ELSE last_active_at END WHERE id = ?",
    )
      .bind(riders, now, riders ?? 0, now, row.id)
      .run();
    if (!row.applied) await applySettings(env, deps, row);
  }
}

/** Free slots whose server nobody has ridden on for `idleDays`. */
async function reclaimIdle(env: Env, deps: Deps, cfg: HostConfig): Promise<void> {
  const cutoff = deps.now() - cfg.idleDays * DAY;
  const idle = await env.DB.prepare("SELECT id FROM host_servers WHERE state = 'ready' AND last_active_at < ?")
    .bind(cutoff)
    .all<{ id: string }>();
  for (const row of idle.results) await deleteServer(env, deps, row.id, `idle ${cfg.idleDays} days`);
}

/**
 * Renewal-aware scale-down. Per pool and region: if the tenants would fit on one box fewer,
 * the emptiest ready box drains (takes no new tenants). A box that has been empty for
 * `emptyDays` and renews within `flagBeforeRenewalDays` is flagged for an operator to cancel
 * at OVH. Nothing here cancels anything.
 */
async function scaleDown(env: Env, deps: Deps, cfg: HostConfig): Promise<void> {
  const now = deps.now();
  const boxes = await env.DB.prepare(
    `SELECT b.*, (SELECT COUNT(*) FROM host_slots s WHERE s.box_id = b.id AND s.server_id IS NOT NULL) AS used
       FROM host_boxes b WHERE b.state IN ('ready', 'draining')`,
  ).all<BoxRow & { used: number }>();

  for (const b of boxes.results) {
    if (b.used > 0 && b.empty_since !== null) {
      await env.DB.prepare("UPDATE host_boxes SET empty_since = NULL WHERE id = ?").bind(b.id).run();
      b.empty_since = null;
    } else if (b.used === 0 && b.empty_since === null) {
      await env.DB.prepare("UPDATE host_boxes SET empty_since = ? WHERE id = ?").bind(now, b.id).run();
      b.empty_since = now;
    }
  }

  const groups = new Map<string, (BoxRow & { used: number })[]>();
  for (const b of boxes.results) {
    const key = `${b.pool}|${b.region}`;
    groups.set(key, [...(groups.get(key) ?? []), b]);
  }
  for (const group of groups.values()) {
    const ready = group.filter((b) => b.state === "ready");
    const used = group.reduce((n, b) => n + b.used, 0);
    const perBox = group[0].slots_total;
    if (ready.length >= 2 && used <= (ready.length - 1) * perBox && !group.some((b) => b.state === "draining")) {
      const emptiest = [...ready].sort((a, z) => a.used - z.used || z.created_at - a.created_at)[0];
      await env.DB.prepare("UPDATE host_boxes SET state = 'draining', stage_at = ? WHERE id = ?").bind(now, emptiest.id).run();
      console.log(JSON.stringify({ msg: "hosting box draining", box: emptiest.id, region: emptiest.region }));
    }
    for (const b of group) {
      if (b.used > 0 || b.empty_since === null || now - b.empty_since < cfg.emptyDays * DAY) continue;
      const renewsSoon = b.renews_at === null || b.renews_at - now <= cfg.flagBeforeRenewalDays * DAY;
      if (!renewsSoon) continue;
      await env.DB.prepare("UPDATE host_boxes SET state = 'flagged', flagged_at = ?, stage_at = ? WHERE id = ?").bind(now, now, b.id).run();
      const by = b.renews_at ? ` before it renews on ${new Date(b.renews_at).toISOString().slice(0, 10)}` : "";
      await alert(
        env,
        deps,
        "flagged",
        `Box ${b.ovh_service ?? b.id} (${b.region}, ${b.pool}) has been empty for ${cfg.emptyDays}+ days. Cancel it at OVH${by}, then mark it cancelled.`,
        { region: b.region, boxId: b.id },
      );
    }
  }
}

export async function hostingTick(env: Env, deps: Deps = defaultDeps(env)): Promise<void> {
  const cfg = hostConfig(env);
  const moving = await env.DB.prepare(
    "SELECT * FROM host_boxes WHERE state IN ('delivering', 'rebuilding', 'installing')",
  ).all<BoxRow>();
  for (const b of moving.results) {
    try {
      await advanceBox(env, deps, cfg, b);
    } catch (err) {
      console.error(JSON.stringify({ msg: "hosting advance", box: b.id, error: String(err) }));
      await env.DB.prepare("UPDATE host_boxes SET last_error = ? WHERE id = ?").bind(String(err).slice(0, 500), b.id).run();
    }
  }
  const steps: [string, () => Promise<void>][] = [
    ["riders", () => pollRiders(env, deps)],
    ["idle", () => reclaimIdle(env, deps, cfg)],
    ["scale", () => scaleDown(env, deps, cfg)],
  ];
  for (const [name, step] of steps) {
    try {
      await step();
    } catch (err) {
      console.error(JSON.stringify({ msg: `hosting ${name}`, error: String(err) }));
    }
  }
}

// ---- Operator ------------------------------------------------------------------------------

export async function operatorView(env: Env): Promise<Result> {
  const cfg = hostConfig(env);
  const regionLabel = (id: string) => regionById(id)?.label ?? id;
  const boxes = await env.DB.prepare(
    `SELECT b.*,
            (SELECT COUNT(*) FROM host_slots s WHERE s.box_id = b.id AND s.server_id IS NOT NULL) AS used,
            (SELECT COUNT(*) FROM host_servers v WHERE v.box_id = b.id AND v.state = 'waiting') AS waiting
       FROM host_boxes b WHERE b.state != 'cancelled' OR b.cancelled_at > ? ORDER BY b.created_at DESC`,
  )
    .bind(Date.now() - 30 * DAY)
    .all<BoxRow & { used: number; waiting: number }>();
  const alerts = await env.DB.prepare("SELECT * FROM host_alerts WHERE acked_at IS NULL ORDER BY created_at DESC LIMIT 100").all<{
    id: string; kind: string; region: string | null; box_id: string | null; message: string; created_at: number; acked_at: number | null;
  }>();
  const invites = await env.DB.prepare("SELECT * FROM host_invites ORDER BY created_at DESC LIMIT 100").all<{
    id: string; steam_id: string | null; quota: number; expires_at: number; claimed_by: string | null; claimed_at: number | null;
    revoked_at: number | null; created_at: number;
  }>();
  const users = await env.DB.prepare("SELECT * FROM host_users ORDER BY created_at DESC").all<{
    steam_id: string; quota: number; suspended: number; created_at: number;
  }>();
  const servers = await env.DB.prepare(
    "SELECT * FROM host_servers WHERE state != 'deleted' ORDER BY created_at DESC",
  ).all<ServerRow>();
  const tracks = await env.DB.prepare("SELECT * FROM host_tracks ORDER BY pool, name").all<{
    id: string; pool: string; name: string; url: string; sha256: string; created_at: number;
  }>();
  return {
    status: 200,
    body: {
      spend: await spend(env, cfg),
      regions: REGIONS.map((r) => ({ id: r.id, label: r.label, datacenter: r.datacenter, location: r.location })),
      boxes: boxes.results.map((b) => ({
        id: b.id,
        pool: b.pool,
        region: b.region,
        regionLabel: regionLabel(b.region),
        datacenter: b.datacenter,
        state: b.state,
        ovhService: b.ovh_service,
        ip: b.ip,
        slotsUsed: b.used,
        slotsTotal: b.slots_total,
        waiting: b.waiting,
        priceUsd: b.price_eur,
        quotedPrice: b.quoted_price,
        quotedCurrency: b.quoted_currency,
        renewsAt: b.renews_at,
        stageAt: b.stage_at,
        emptySince: b.empty_since,
        flaggedAt: b.flagged_at,
        lastError: b.last_error,
        createdAt: b.created_at,
      })),
      alerts: alerts.results.map((a) => ({
        id: a.id, kind: a.kind, region: a.region, boxId: a.box_id, message: a.message, createdAt: a.created_at, ackedAt: a.acked_at,
      })),
      // The code itself is never stored, so it can't be shown again; only the link at minting.
      invites: invites.results.map((i) => ({
        id: i.id, steamId: i.steam_id, quota: i.quota, used: i.claimed_by !== null, claimedBy: i.claimed_by,
        expiresAt: i.expires_at, revokedAt: i.revoked_at, createdAt: i.created_at,
      })),
      users: users.results.map((u) => ({ steamId: u.steam_id, quota: u.quota, suspended: u.suspended === 1, createdAt: u.created_at })),
      servers: servers.results.map((v) => ({
        id: v.id, ownerSteamId: v.steam_id, name: v.name, type: v.type, region: v.region, regionLabel: regionLabel(v.region),
        boxId: v.box_id, state: v.state, riders: v.riders, lastActiveAt: v.last_active_at, createdAt: v.created_at,
      })),
      tracks: tracks.results.map((t) => ({ id: t.id, pool: t.pool, name: t.name, url: t.url, sha256: t.sha256, createdAt: t.created_at })),
    },
  };
}

export async function operatorBox(env: Env, deps: Deps, id: string, action: string): Promise<Result> {
  const b = await box(env, id);
  if (!b) return { status: 404, body: { error: "no such box" } };
  const now = deps.now();
  if (action === "drain") {
    if (b.state !== "ready") return { status: 409, body: { error: `box is ${b.state}` } };
    await env.DB.prepare("UPDATE host_boxes SET state = 'draining', stage_at = ? WHERE id = ?").bind(now, id).run();
  } else if (action === "undrain") {
    if (b.state !== "draining" && b.state !== "flagged") return { status: 409, body: { error: `box is ${b.state}` } };
    await env.DB.prepare("UPDATE host_boxes SET state = 'ready', flagged_at = NULL, stage_at = ? WHERE id = ?").bind(now, id).run();
  } else if (action === "cancelled") {
    const used = await env.DB.prepare("SELECT COUNT(*) AS n FROM host_slots WHERE box_id = ? AND server_id IS NOT NULL").bind(id).first<{ n: number }>();
    if ((used?.n ?? 0) > 0) return { status: 409, body: { error: "the box still has servers on it" } };
    await env.DB.prepare("UPDATE host_boxes SET state = 'cancelled', cancelled_at = ?, stage_at = ? WHERE id = ?").bind(now, now, id).run();
    await failWaiting(env, id);
    await env.DB.prepare("UPDATE host_alerts SET acked_at = ? WHERE box_id = ? AND acked_at IS NULL").bind(now, id).run();
  } else if (action === "retry") {
    if (b.state !== "failed" || !b.ovh_service) return { status: 409, body: { error: "only a delivered box that failed can be retried" } };
    await env.DB.prepare("UPDATE host_boxes SET state = 'rebuilding', stage_at = ?, last_error = NULL WHERE id = ?").bind(now - 5 * MINUTE, id).run();
  } else {
    return { status: 400, body: { error: "unknown box action" } };
  }
  console.log(JSON.stringify({ msg: "hosting box action", box: id, action }));
  return { status: 200, body: { ok: true } };
}

export async function addTrack(env: Env, deps: Deps, input: Record<string, unknown>): Promise<Result> {
  const pool = input.pool;
  const name = cleanName(input.name);
  const url = typeof input.url === "string" ? input.url.trim() : "";
  const sha256 = typeof input.sha256 === "string" ? input.sha256.trim().toLowerCase() : "";
  if (pool !== "native" && pool !== "legacy") return { status: 400, body: { error: "pool must be native or legacy" } };
  if (!name) return { status: 400, body: { error: "name is required" } };
  if (!/^https:\/\/[^\s]+$/.test(url) || url.length > 500) return { status: 400, body: { error: "url must be https" } };
  if (!/^[0-9a-f]{64}$/.test(sha256)) return { status: 400, body: { error: "sha256 must be 64 hex characters" } };
  // Also the file name on the box (`/etc/mxbserver/tracks/<id>.pkz`), so it stays plain.
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 40) || "track";
  const id = `${pool === "native" ? "n" : "l"}-${slug}`;
  try {
    await env.DB.prepare("INSERT INTO host_tracks (id, pool, name, url, sha256, created_at) VALUES (?, ?, ?, ?, ?, ?)")
      .bind(id, pool, name, url, sha256, deps.now())
      .run();
  } catch {
    return { status: 409, body: { error: "a track with that name already exists" } };
  }
  return { status: 201, body: { id } };
}
