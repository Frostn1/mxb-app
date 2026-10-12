/**
 * Builds from the Build page (mxbsecure.com/build): a signed-in rider keeps the bikes they built, and
 * sends one as a model swap order for the swap creator to make.
 *
 * A build is the page's spec sheet (`mxb-build/1`): a bike and one part per category, with options and
 * colours. It names catalogue ids only: this side never learns where a part's real files are, and nothing
 * it stores is a deliverable. The finished swap ships through the locked-asset flow like every other
 * paid file.
 *
 * Stages follow the DGS ticket flow: `saved` until sent, then quote -> pending_payment -> paid ->
 * approval -> complete, one step at a time, or cancelled. The creator quotes in USD and confirms payment
 * by hand; nothing is charged here. Moving on from `approval` needs the rider's latest answer to be yes.
 */

import { cors, refuseCrossSiteWrite } from "./assets";
import { isBanned } from "./bans";
import { clarenceConfig, discordName, postToClarence, SIGNATURE_HEADER, ticketUrl, verify, type OrderEvent } from "./clarence";
import { isWebAdmin } from "./webadmin";
import { webSession } from "./websession";

export const SPEC_FORMAT = "mxb-build/1";

export const STAGES = ["saved", "quote", "pending_payment", "paid", "approval", "complete", "cancelled"] as const;
export type Stage = (typeof STAGES)[number];
/** The creator's path through a sent build. */
const FLOW: Stage[] = ["quote", "pending_payment", "paid", "approval", "complete"];

/** The spec as JSON. A full bike is a few kilobytes. */
export const MAX_SPEC_BYTES = 24 * 1024;
/** The build picture, decoded. */
export const MAX_SHOT_BYTES = 1_500_000;
/** The whole request: the spec plus the picture as base64. */
export const MAX_BODY_BYTES = MAX_SPEC_BYTES + Math.ceil((MAX_SHOT_BYTES * 4) / 3) + 2048;
/** Builds one account may save per day. Bounds a script, not a rider. */
export const MAX_BUILDS_PER_DAY = 30;
/** Builds one account may send per day. */
export const MAX_SENT_PER_DAY = 5;
export const MAX_PARTS = 64;
export const MAX_OPTIONS = 12;
export const MAX_FILES_PER_PART = 24;
export const MAX_NOTE_CHARS = 500;
export const MAX_GUIDS = 10;

const ID_RE = /^[a-z0-9][a-z0-9-]{0,63}$/;
const REF_RE = /^MXB-[0-9A-Z]{4,12}$/;
const BUILD_RE = /^\/v1\/web\/builds\/([0-9a-f-]{36})(\/shot|\/send|\/approve|\/stage|\/discard|\/clarence)?$/i;
const CLARENCE_RE = /^\/v1\/clarence\/builds\/([0-9a-f-]{36})$/i;
/** Where a rider follows their orders. */
const MINE_PAGE = "https://mxbsecure.com/build/mine";
const SHOT_PREFIX = "builds/";

export interface SpecPart {
  category: string;
  categoryLabel: string;
  group: string;
  id: string;
  brand: string;
  model: string;
  by?: string;
  colourway?: string;
  options?: { label: string; value: string }[];
  page?: string;
  files?: string[];
}

export interface Spec {
  format: typeof SPEC_FORMAT;
  ref: string;
  title: string;
  created: string;
  bike: { id: string; model: string; by: string };
  parts: SpecPart[];
  stock: string[];
}

export function isWebBuildsPath(path: string): boolean {
  return path === "/v1/web/builds" || path.startsWith("/v1/web/builds/");
}

export function isClarencePath(path: string): boolean {
  return path.startsWith("/v1/clarence/");
}

/**
 * `POST /v1/clarence/builds/:id`, signed by Clarence as its ticket moves:
 *
 *   { stage?, priceCents?, note?, approval?: "approved" | "changes", approvalNote?, ticketUrl? }
 *
 * `stage` goes through the same one-step rules as the site's queue; `approval` is the customer's
 * answer pressed in the ticket; `ticketUrl` links the ticket from the rider's page.
 */
export async function clarenceRoutes(request: Request, url: URL, env: Env): Promise<Response> {
  const say = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
  const cfg = clarenceConfig(env);
  if (!cfg) return say(503, { error: "Clarence isn't connected." });
  const m = CLARENCE_RE.exec(url.pathname);
  if (!m) return say(404, { error: "no such endpoint" });
  if (request.method !== "POST") return say(405, { error: "method not allowed" });
  const raw = await request.text();
  if (raw.length > 8192) return say(413, { error: "too large" });
  if (!(await verify(cfg.secret, raw, request.headers.get(SIGNATURE_HEADER)))) return say(401, { error: "bad signature" });
  let body: Record<string, unknown>;
  try {
    const b = JSON.parse(raw) as unknown;
    body = b && typeof b === "object" && !Array.isArray(b) ? (b as Record<string, unknown>) : {};
  } catch {
    return say(400, { error: "not JSON" });
  }
  let row = await one(env, m[1]);
  if (!row || row.stage === "saved") return say(404, { error: "no such order" });
  const now = Date.now();

  if (body.ticketUrl !== undefined) {
    const link = ticketUrl(body.ticketUrl);
    if (!link) return say(400, { error: "ticketUrl is a discord.com/channels link" });
    await env.DB.prepare("UPDATE builds SET ticket_url = ?, clarence = 'sent', updated_at = ? WHERE id = ?").bind(link, now, row.id).run();
  }
  if (body.approval !== undefined) {
    if (body.approval !== "approved" && body.approval !== "changes") return say(400, { error: "approval is approved or changes" });
    if (row.stage !== "approval" || row.approval !== "asked") return say(409, { error: "nothing to approve" });
    const note = optText(body.approvalNote, MAX_NOTE_CHARS);
    if (note === false) return say(400, { error: `approvalNote is at most ${MAX_NOTE_CHARS} characters` });
    await env.DB.prepare("UPDATE builds SET approval = ?, approval_note = ?, updated_at = ? WHERE id = ?").bind(body.approval, note ?? null, now, row.id).run();
    row = (await one(env, row.id))!;
  }
  if (body.stage !== undefined && body.stage !== row.stage) {
    const r = await setStage(env, row, body, now);
    if (r.status !== 200) return say(r.status, r.body);
  } else if (body.stage === undefined && (body.priceCents !== undefined || body.note !== undefined)) {
    return say(400, { error: "priceCents and note go with a stage" });
  }
  const after = (await one(env, row.id))!;
  return say(200, { id: after.id, stage: after.stage, approval: after.approval });
}

/** Printable text, one line, capped. Control characters go. */
function text(v: unknown, max: number): string | null {
  if (typeof v !== "string") return null;
  const s = v.replace(/[\u0000-\u001f\u007f]+/g, " ").trim();
  return s && s.length <= max ? s : null;
}

/** Optional text: absent is fine, present must be good. `false` means bad. */
function optText(v: unknown, max: number): string | undefined | false {
  if (v === undefined || v === null || v === "") return undefined;
  return text(v, max) ?? false;
}

/** The build a rider sent, or why it was refused. Unknown fields are dropped, not kept. */
export function parseSpec(raw: unknown): { spec: Spec } | { error: string } {
  if (!raw || typeof raw !== "object") return { error: "spec must be an object" };
  const r = raw as Record<string, unknown>;
  if (r.format !== SPEC_FORMAT) return { error: `format must be ${SPEC_FORMAT}` };
  const ref = typeof r.ref === "string" && REF_RE.test(r.ref) ? r.ref : null;
  if (!ref) return { error: "ref is missing" };
  const title = text(r.title, 120);
  const created = typeof r.created === "string" && !Number.isNaN(Date.parse(r.created)) ? new Date(r.created).toISOString() : null;
  if (!title || !created) return { error: "spec needs a title and when it was made" };
  const bike = (r.bike ?? {}) as Record<string, unknown>;
  const bikeId = typeof bike.id === "string" && ID_RE.test(bike.id) ? bike.id : null;
  const model = text(bike.model, 80);
  const by = text(bike.by, 80);
  if (!bikeId || !model || !by) return { error: "bike needs an id, a model and who made it" };
  if (!Array.isArray(r.parts) || r.parts.length > MAX_PARTS) return { error: "parts must be a list" };

  const parts: SpecPart[] = [];
  const seen = new Set<string>();
  for (const p of r.parts as unknown[]) {
    const q = (p ?? {}) as Record<string, unknown>;
    const category = typeof q.category === "string" && ID_RE.test(q.category) ? q.category : null;
    if (!category) return { error: "every part needs a category" };
    if (seen.has(category)) return { error: `one part per category (${category} twice)` };
    seen.add(category);
    const id = typeof q.id === "string" && ID_RE.test(q.id) ? q.id : null;
    const categoryLabel = text(q.categoryLabel, 60);
    const group = text(q.group, 60);
    const brand = text(q.brand, 60);
    const partModel = text(q.model, 80);
    if (!id || !categoryLabel || !group || !brand || !partModel) return { error: `${category}: part needs an id, brand and model` };
    const part: SpecPart = { category, categoryLabel, group, id, brand, model: partModel };
    const partBy = optText(q.by, 80);
    const colourway = optText(q.colourway, 80);
    if (partBy === false || colourway === false) return { error: `${category}: bad text` };
    if (partBy) part.by = partBy;
    if (colourway) part.colourway = colourway;
    if (q.options !== undefined) {
      if (!Array.isArray(q.options) || q.options.length > MAX_OPTIONS) return { error: `${category}: bad options` };
      const options: { label: string; value: string }[] = [];
      for (const o of q.options as unknown[]) {
        const x = (o ?? {}) as Record<string, unknown>;
        const label = text(x.label, 40);
        const value = text(x.value, 80);
        if (!label || !value) return { error: `${category}: options are label and value` };
        options.push({ label, value });
      }
      part.options = options;
    }
    if (q.page !== undefined) {
      // Only our own pages: the sheet links the shop, nothing else.
      if (typeof q.page !== "string" || !/^https:\/\/mxbsecure\.com\/[a-z0-9/_-]{1,120}$/.test(q.page)) return { error: `${category}: bad page` };
      part.page = q.page;
    }
    if (q.files !== undefined) {
      if (!Array.isArray(q.files) || q.files.length > MAX_FILES_PER_PART) return { error: `${category}: bad files` };
      const files: string[] = [];
      for (const f of q.files as unknown[]) {
        const name = text(f, 120);
        if (!name || /[\\/]/.test(name)) return { error: `${category}: files are names, not paths` };
        files.push(name);
      }
      part.files = files;
    }
    parts.push(part);
  }
  if (!Array.isArray(r.stock) || r.stock.length > MAX_PARTS) return { error: "stock must be a list" };
  const stock: string[] = [];
  for (const s of r.stock as unknown[]) {
    const t = text(s, 60);
    if (!t) return { error: "stock is a list of categories" };
    stock.push(t);
  }
  return { spec: { format: SPEC_FORMAT, ref, title, created, bike: { id: bikeId, model, by }, parts, stock } };
}

const SHOT_TYPES: Record<string, { ext: string; magic: (b: Uint8Array) => boolean }> = {
  "image/webp": {
    ext: "webp",
    magic: (b) => String.fromCharCode(...b.slice(0, 4)) === "RIFF" && String.fromCharCode(...b.slice(8, 12)) === "WEBP",
  },
  "image/png": { ext: "png", magic: (b) => b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47 },
  "image/jpeg": { ext: "jpg", magic: (b) => b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff },
};

/** The build picture from a `data:` URL: its bytes and type, or why it was refused. */
export function parseShot(raw: unknown): { bytes: Uint8Array; type: string; ext: string } | { error: string } {
  if (typeof raw !== "string") return { error: "shot must be a data URL" };
  const m = /^data:(image\/(?:webp|png|jpeg));base64,([A-Za-z0-9+/=]+)$/.exec(raw);
  if (!m) return { error: "shot must be a WebP, PNG or JPEG data URL" };
  if (Math.floor((m[2].length * 3) / 4) > MAX_SHOT_BYTES + 3) return { error: "shot is too large" };
  let bytes: Uint8Array;
  try {
    bytes = Uint8Array.from(atob(m[2]), (c) => c.charCodeAt(0));
  } catch {
    return { error: "shot is not valid base64" };
  }
  const t = SHOT_TYPES[m[1]];
  if (bytes.length > MAX_SHOT_BYTES || bytes.length < 12 || !t.magic(bytes)) return { error: "shot is not that kind of image" };
  return { bytes, type: m[1], ext: t.ext };
}

interface Row {
  id: string;
  steam_id: string;
  name: string;
  bike: string;
  ref: string;
  spec: string;
  shot_key: string | null;
  stage: Stage;
  guids: number;
  note: string | null;
  price_cents: number | null;
  admin_note: string | null;
  approval: string | null;
  approval_note: string | null;
  sent_at: number | null;
  created_at: number;
  updated_at: number;
  discord: string | null;
  clarence: string | null;
  ticket_url: string | null;
}

function view(r: Row, admin: boolean) {
  return {
    id: r.id,
    ref: r.ref,
    bike: r.bike,
    spec: JSON.parse(r.spec) as Spec,
    hasShot: r.shot_key !== null,
    stage: r.stage,
    guids: r.guids,
    note: r.note,
    priceCents: r.price_cents,
    // The creator's note is for the rider too: it is how the quote and changes are explained.
    adminNote: r.admin_note,
    approval: r.approval,
    approvalNote: r.approval_note,
    sentAt: r.sent_at,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
    discord: r.discord,
    ticketUrl: r.ticket_url,
    ...(admin ? { steamId: r.steam_id, name: r.name, clarence: r.clarence } : {}),
  };
}

function orderEvent(r: Row, event: OrderEvent["event"]): OrderEvent {
  return {
    event,
    id: r.id,
    ref: r.ref,
    steamId: r.steam_id,
    steamName: r.name,
    discord: r.discord,
    guids: r.guids,
    note: r.note,
    stage: r.stage,
    priceCents: r.price_cents,
    approval: r.approval,
    approvalNote: r.approval_note,
    spec: JSON.parse(r.spec) as Spec,
    page: MINE_PAGE,
    sentAt: r.sent_at,
  };
}

/**
 * Tell Clarence about an order, when the feed is on, and record whether it took it. A failure
 * never fails the rider's request: the queue shows it and the creator can resend.
 */
export async function notifyClarence(env: Env, id: string, event: OrderEvent["event"], fetchImpl: typeof fetch = fetch): Promise<void> {
  const cfg = clarenceConfig(env);
  if (!cfg) return;
  const row = await one(env, id);
  if (!row || row.stage === "saved") return;
  const ok = await postToClarence(cfg, orderEvent(row, event), fetchImpl);
  // An approval answer that didn't arrive leaves the order itself in Clarence: only the first
  // delivery decides 'failed'.
  if (event === "order.created" || ok) {
    await env.DB.prepare("UPDATE builds SET clarence = ? WHERE id = ?").bind(ok ? "sent" : "failed", id).run();
  }
}

function guidsOf(v: unknown): number | null {
  if (v === undefined || v === null) return 1;
  return typeof v === "number" && Number.isInteger(v) && v >= 1 && v <= MAX_GUIDS ? v : null;
}

async function sentToday(env: Env, steamId: string, now: number): Promise<number> {
  const r = await env.DB.prepare("SELECT COUNT(*) AS n FROM builds WHERE steam_id = ? AND sent_at > ?")
    .bind(steamId, now - 86_400_000)
    .first<{ n: number }>();
  return r?.n ?? 0;
}

/** Save a build, and send it straight away when `send` is true. */
export async function createBuild(
  env: Env,
  who: { steamId: string; name: string },
  body: Record<string, unknown>,
  now = Date.now(),
): Promise<{ status: number; body: unknown }> {
  const parsed = parseSpec(body.spec);
  if ("error" in parsed) return { status: 400, body: { error: parsed.error } };
  const specJson = JSON.stringify(parsed.spec);
  if (specJson.length > MAX_SPEC_BYTES) return { status: 413, body: { error: "spec is too large" } };
  const send = body.send === true;
  if (send && parsed.spec.parts.length === 0) return { status: 400, body: { error: "pick at least one part to order" } };
  const guids = guidsOf(body.guids);
  if (guids === null) return { status: 400, body: { error: `guids is 1 to ${MAX_GUIDS}` } };
  const note = optText(body.note, MAX_NOTE_CHARS);
  if (note === false) return { status: 400, body: { error: `note is at most ${MAX_NOTE_CHARS} characters` } };
  const discord = discordName(body.discord);
  if (discord === false) return { status: 400, body: { error: "That isn't a Discord username." } };
  let shot: { bytes: Uint8Array; type: string; ext: string } | null = null;
  if (body.shot !== undefined && body.shot !== null) {
    const s = parseShot(body.shot);
    if ("error" in s) return { status: 400, body: { error: s.error } };
    shot = s;
  }
  const today = await env.DB.prepare("SELECT COUNT(*) AS n FROM builds WHERE steam_id = ? AND created_at > ?")
    .bind(who.steamId, now - 86_400_000)
    .first<{ n: number }>();
  if ((today?.n ?? 0) >= MAX_BUILDS_PER_DAY) return { status: 429, body: { error: "That's enough builds for today." } };
  if (send && (await sentToday(env, who.steamId, now)) >= MAX_SENT_PER_DAY) {
    return { status: 429, body: { error: "That's enough orders for today." } };
  }

  const id = crypto.randomUUID();
  const shotKey = shot ? `${SHOT_PREFIX}${id}.${shot.ext}` : null;
  if (shot && shotKey) await env.ASSET_LOCKED.put(shotKey, shot.bytes, { httpMetadata: { contentType: shot.type } });
  const stage: Stage = send ? "quote" : "saved";
  await env.DB.prepare(
    `INSERT INTO builds (id, steam_id, name, bike, ref, spec, shot_key, stage, guids, note, discord, sent_at, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  )
    .bind(id, who.steamId, who.name.slice(0, 80), parsed.spec.bike.id, parsed.spec.ref, specJson, shotKey, stage, guids, note ?? null, discord, send ? now : null, now, now)
    .run();
  console.log(JSON.stringify({ msg: "build", id, stage, bike: parsed.spec.bike.id, parts: parsed.spec.parts.length }));
  return { status: 201, body: { id, ref: parsed.spec.ref, stage } };
}

/** Send a saved build as an order. */
export async function sendBuild(env: Env, row: Row, body: Record<string, unknown>, now = Date.now()) {
  if (row.stage !== "saved") return { status: 409, body: { error: "already sent" } };
  const spec = JSON.parse(row.spec) as Spec;
  if (spec.parts.length === 0) return { status: 400, body: { error: "pick at least one part to order" } };
  const guids = body.guids === undefined ? row.guids : guidsOf(body.guids);
  if (guids === null) return { status: 400, body: { error: `guids is 1 to ${MAX_GUIDS}` } };
  const note = optText(body.note, MAX_NOTE_CHARS);
  if (note === false) return { status: 400, body: { error: `note is at most ${MAX_NOTE_CHARS} characters` } };
  const discord = discordName(body.discord);
  if (discord === false) return { status: 400, body: { error: "That isn't a Discord username." } };
  if ((await sentToday(env, row.steam_id, now)) >= MAX_SENT_PER_DAY) return { status: 429, body: { error: "That's enough orders for today." } };
  await env.DB.prepare(
    "UPDATE builds SET stage = 'quote', guids = ?, note = COALESCE(?, note), discord = COALESCE(?, discord), sent_at = ?, updated_at = ? WHERE id = ? AND stage = 'saved'",
  )
    .bind(guids, note ?? null, discord, now, now, row.id)
    .run();
  return { status: 200, body: { id: row.id, stage: "quote" } };
}

/** The rider's answer to the creator's approval request. */
export async function answerApproval(env: Env, row: Row, body: Record<string, unknown>, now = Date.now()) {
  if (row.stage !== "approval" || row.approval !== "asked") return { status: 409, body: { error: "nothing to approve" } };
  if (typeof body.approve !== "boolean") return { status: 400, body: { error: "approve must be true or false" } };
  const note = optText(body.note, MAX_NOTE_CHARS);
  if (note === false) return { status: 400, body: { error: `note is at most ${MAX_NOTE_CHARS} characters` } };
  if (!body.approve && !note) return { status: 400, body: { error: "say what to change" } };
  const approval = body.approve ? "approved" : "changes";
  await env.DB.prepare("UPDATE builds SET approval = ?, approval_note = ?, updated_at = ? WHERE id = ?")
    .bind(approval, note ?? null, now, row.id)
    .run();
  return { status: 200, body: { id: row.id, approval } };
}

/**
 * The creator moves a sent build one step along, or cancels it. Quoting needs a price; leaving
 * `approval` needs the rider's yes; `approval` itself (again) asks the rider anew.
 */
export async function setStage(env: Env, row: Row, body: Record<string, unknown>, now = Date.now()) {
  const to = body.stage;
  if (typeof to !== "string" || !(STAGES as readonly string[]).includes(to) || to === "saved") {
    return { status: 400, body: { error: `stage must be one of ${FLOW.join(", ")} or cancelled` } };
  }
  const adminNote = optText(body.note, MAX_NOTE_CHARS);
  if (adminNote === false) return { status: 400, body: { error: `note is at most ${MAX_NOTE_CHARS} characters` } };
  let price = row.price_cents;
  if (body.priceCents !== undefined) {
    const p = body.priceCents;
    if (typeof p !== "number" || !Number.isInteger(p) || p < 0 || p > 1_000_000) return { status: 400, body: { error: "priceCents is whole cents" } };
    price = p;
  }
  if (row.stage === "saved") return { status: 409, body: { error: "not sent yet" } };
  if (row.stage === "complete" || row.stage === "cancelled") return { status: 409, body: { error: `already ${row.stage}` } };
  let approval = row.approval;
  if (to !== "cancelled") {
    const from = FLOW.indexOf(row.stage);
    const next = FLOW.indexOf(to as Stage);
    const again = to === "approval" && row.stage === "approval";
    if (next !== from + 1 && !again) return { status: 409, body: { error: `${row.stage} goes to ${FLOW[from + 1]}` } };
    if (to === "pending_payment" && price === null) return { status: 400, body: { error: "quote a price first" } };
    if (to === "approval") approval = "asked";
    if (to === "complete" && row.approval !== "approved") return { status: 409, body: { error: "waiting on the rider's approval" } };
  }
  await env.DB.prepare(
    "UPDATE builds SET stage = ?, price_cents = ?, approval = ?, approval_note = CASE WHEN ? THEN NULL ELSE approval_note END, admin_note = COALESCE(?, admin_note), updated_at = ? WHERE id = ?",
  )
    .bind(to, price, approval, to === "approval" ? 1 : 0, adminNote ?? null, now, row.id)
    .run();
  return { status: 200, body: { id: row.id, stage: to } };
}

export async function listBuilds(env: Env, opts: { steamId?: string; stage?: string; sent?: boolean }) {
  const where: string[] = [];
  const args: unknown[] = [];
  if (opts.steamId) {
    where.push("steam_id = ?");
    args.push(opts.steamId);
  }
  if (opts.sent) where.push("stage != 'saved'");
  if (opts.stage && (STAGES as readonly string[]).includes(opts.stage)) {
    where.push("stage = ?");
    args.push(opts.stage);
  }
  const rows = await env.DB.prepare(
    `SELECT * FROM builds ${where.length ? `WHERE ${where.join(" AND ")}` : ""} ORDER BY COALESCE(sent_at, created_at) DESC LIMIT 200`,
  )
    .bind(...args)
    .all<Row>();
  return rows.results;
}

async function one(env: Env, id: string): Promise<Row | null> {
  return env.DB.prepare("SELECT * FROM builds WHERE id = ?").bind(id.toLowerCase()).first<Row>();
}

async function jsonBody(request: Request, max: number): Promise<Record<string, unknown> | null> {
  const len = Number(request.headers.get("Content-Length") ?? 0);
  if (len > max) return null;
  const raw = await request.text();
  if (raw.length > max) return null;
  try {
    const b = JSON.parse(raw || "{}") as unknown;
    return b && typeof b === "object" && !Array.isArray(b) ? (b as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/**
 * `/v1/web/builds*`: a signed-in rider's builds and orders, and the swap creator's queue.
 *
 *   POST /v1/web/builds               save one: { spec, shot?, send?, guids?, note? }
 *   GET  /v1/web/builds               mine; an admin adds ?all=1 (and ?stage=) for the queue of sent ones
 *   GET  /v1/web/builds/:id           one, mine or (admin) anybody's
 *   GET  /v1/web/builds/:id/shot      its picture
 *   POST /v1/web/builds/:id/send      mine, saved -> quote: { guids?, note? }
 *   POST /v1/web/builds/:id/approve   mine, at approval: { approve, note? }
 *   POST /v1/web/builds/:id/discard   mine, while saved: gone
 *   POST /v1/web/builds/:id/stage     admin: { stage, priceCents?, note? }
 */
export async function webBuildRoutes(request: Request, url: URL, env: Env, origin: string | null): Promise<Response> {
  const say = (status: number, body: unknown) =>
    cors(new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } }), origin);
  if (request.method === "OPTIONS") {
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, OPTIONS", "Content-Type");
  }
  const session = await webSession(request, env);
  if (!session) return say(401, { error: "not signed in" });
  const offSite = refuseCrossSiteWrite(request, env);
  if (offSite) return cors(offSite, origin);
  const admin = isWebAdmin(session.steamId, env);
  const path = url.pathname;

  if (path === "/v1/web/builds") {
    if (request.method === "POST") {
      if (await isBanned(env, { steamId: session.steamId })) return say(403, { error: "banned" });
      const body = await jsonBody(request, MAX_BODY_BYTES);
      if (!body) return say(413, { error: "too large" });
      const r = await createBuild(env, session, body);
      const made = r.body as { id?: string; stage?: string };
      if (r.status === 201 && made.stage === "quote" && made.id) await notifyClarence(env, made.id, "order.created");
      return say(r.status, r.body);
    }
    if (request.method === "GET") {
      const all = admin && url.searchParams.get("all") === "1";
      const rows = await listBuilds(env, {
        steamId: all ? undefined : session.steamId,
        sent: all,
        stage: url.searchParams.get("stage") ?? undefined,
      });
      return say(200, { builds: rows.map((r) => view(r, all)) });
    }
    return say(405, { error: "method not allowed" });
  }

  const m = BUILD_RE.exec(path);
  if (!m) return say(404, { error: "no such endpoint" });
  const row = await one(env, m[1]);
  const mine = row?.steam_id === session.steamId;
  // Somebody else's build reads as no build at all; the creator sees only the sent ones.
  if (!row || (!mine && !(admin && row.stage !== "saved"))) return say(404, { error: "no such build" });
  const action = m[2]?.toLowerCase();

  if (action === undefined || action === "/shot") {
    if (request.method !== "GET") return say(405, { error: "method not allowed" });
    if (action === undefined) return say(200, { build: view(row, admin && !mine) });
    const obj = row.shot_key ? await env.ASSET_LOCKED.get(row.shot_key) : null;
    if (!obj) return say(404, { error: "no picture" });
    return cors(
      new Response(obj.body, {
        headers: { "content-type": obj.httpMetadata?.contentType ?? "image/webp", "cache-control": "private, no-store" },
      }),
      origin,
    );
  }

  if (request.method !== "POST") return say(405, { error: "method not allowed" });
  const body = await jsonBody(request, 4096);
  if (!body) return say(413, { error: "too large" });
  if (action === "/stage") {
    if (!admin) return say(403, { error: "not an admin" });
    const r = await setStage(env, row, body);
    return say(r.status, r.body);
  }
  if (action === "/clarence") {
    // Resend an order Clarence didn't take.
    if (!admin) return say(403, { error: "not an admin" });
    if (!clarenceConfig(env)) return say(503, { error: "Clarence isn't connected." });
    await notifyClarence(env, row.id, "order.created");
    const after = await one(env, row.id);
    return say(after?.clarence === "sent" ? 200 : 502, { id: row.id, clarence: after?.clarence ?? null });
  }
  if (!mine) return say(404, { error: "no such build" });
  if (action === "/send") {
    if (await isBanned(env, { steamId: session.steamId })) return say(403, { error: "banned" });
    const r = await sendBuild(env, row, body);
    if (r.status === 200) await notifyClarence(env, row.id, "order.created");
    return say(r.status, r.body);
  }
  if (action === "/approve") {
    const r = await answerApproval(env, row, body);
    if (r.status === 200) await notifyClarence(env, row.id, "order.approval");
    return say(r.status, r.body);
  }
  // discard: only a build that was never sent; an order stays on record.
  if (row.stage !== "saved") return say(409, { error: "sent builds stay on record" });
  if (row.shot_key) await env.ASSET_LOCKED.delete(row.shot_key);
  await env.DB.prepare("DELETE FROM builds WHERE id = ? AND stage = 'saved'").bind(row.id).run();
  return say(200, { id: row.id, discarded: true });
}
