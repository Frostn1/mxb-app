/**
 * The mod mirror's sync: mxb-mods.com's catalogue, walked politely and copied into R2.
 *
 * Three steps, each with its own budget per cron run so one run can never blow the Worker's
 * CPU or subrequest limits, and each resumable from D1 alone:
 *
 *  1. **Discover.** Walk the REST listing of everything modified since the last finished walk
 *     (`Listing`). A new or changed post is upserted and its page marked due. A second, cheap
 *     cursor sweeps every post id so `last_seen` stays true and deleted posts drop out of search.
 *  2. **Read pages.** The download links, version, byline, description and pictures are only
 *     on the rendered page, so each due page is read once, parsed the way the app parses it
 *     (`apps/manager/src-tauri/src/mods/mxb.rs`), and its options upserted. An unchanged link
 *     keeps its mirrored file; a changed one is fetched again. The post's own words become
 *     data (`modbody.ts`) and its content images are copied to `img/<sha256>.<ext>`.
 *     The cron doesn't read pages itself: it leases due rows and hands them to the
 *     `mxb-mirror` queue (`dispatchPages`), whose consumer reads a batch on a few lanes at
 *     once (`readPageJobs`). A row read by an older parser (`PAGE_REV`) is read again, after
 *     the new ones.
 *  3. **Dispatch.** Files that are due are leased and handed to the `mxb-mirror` queue, one
 *     message per file. The consumer (`mirrorfetch.ts`) streams each into R2 by SHA-256.
 *
 * Everything sent to mxb-mods.com carries a named user agent, is spaced out, and is checked
 * against the site's robots.txt first. A 403, 429 or 503 stops every lane and cools the whole
 * sync down: at least Retry-After, ten minutes the first time, doubling while it keeps
 * happening, up to two hours.
 *
 * The rate is the gentle one: one lane, one message a batch, one consumer invocation at a time
 * (the queue's `max_batch_size` and `max_concurrency`), and every request to the site at least
 * 3 s after the one before, pictures included, an invocation's first too. A post is one page
 * and about three pictures, ~12 s, so ~300 posts an hour. The cron walks four listing pages
 * and one sweep page a run, 3 s apart. The cron logs the measured rate.
 */

import { decodeEntities } from "./trackcatalog";
import { evictUnused, wantLiveTracks } from "./mirrorpolicy";
import { newPublicId } from "./modids";
import { fetcherRouter, pagesViaFetcher } from "./fetcherroute";
import { isModsHost, MAX_IMAGE_BYTES, MAX_IMAGES, parsePostBody, type PostImage } from "./modbody";

export const UA = "mxbsecure-mirror/1 (+https://mxbsecure.com/mods)";
/** The product token robots.txt groups are matched against. */
const ROBOTS_TOKEN = "mxbsecure-mirror";
const MODS_BASE = "https://mxb-mods.com";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** The listing walk stops after this many pages or this long, whichever comes first; the next
 *  run resumes it. ~290 pages of 50 cover the whole catalogue. */
const DISCOVER_BUDGET_MS = 6 * MINUTE;
const LIST_MAX_PAGES_PER_RUN = 4;
const LIST_PER_PAGE = 50;
/** Id-sweep pages per run, 100 ids each. */
const SWEEP_PAGES_PER_RUN = 1;
/** Post pages read per run when there is no queue to hand them to. */
const PAGES_PER_RUN = 40;
/** Page reads kept leased on the queue: each run tops it up to this. At ~5 posts a minute it
 *  is about twelve minutes of work: the queue doesn't run dry between two runs, and it drains
 *  well inside the lease, so nothing is sent twice. */
const PAGE_QUEUE_TARGET = 60;
/** How long a queued page read stays leased before it is assumed lost and sent again. */
export const PAGE_LEASE_MS = 60 * MINUTE;
/** The page parser. A row read by an older one is read again (rows from before pictures: 0). */
export const PAGE_REV = 1;
/** Pages read side by side in one consumer invocation. One: the gentle rate. */
export const PAGE_LANES = 1;
/** Files handed to the queue per run. The queue consumer's own concurrency is the other cap. */
const DISPATCH_PER_RUN = 20;
/** Pause between two of the cron's own requests to mxb-mods.com (listing, sweep, categories). */
const SPACING_MS = 3000;
/** Pause before every page request on a lane, and between two pictures. */
export const PAGE_SPACING_MS = 3000;
const IMAGE_SPACING_MS = 3000;
/** How long the sync leaves the site alone after it refuses a request: the first time, and
 *  at most. Doubles while refusals keep coming. Retry-After wins when it asks for longer. */
const COOLDOWN_MIN_MS = 10 * MINUTE;
const COOLDOWN_MAX_MS = 2 * HOUR;
/** How long a dispatched file stays leased before it is assumed lost and sent again. */
export const LEASE_MS = 45 * MINUTE;
/** A post not seen by a complete id sweep for this long is hidden from search. */
const GONE_AFTER_MS = 14 * DAY;
const ROBOTS_TTL_MS = DAY;
const CATEGORY_TTL_MS = DAY;
const PAGE_MAX_ATTEMPTS = 6;
const MAX_DESCRIPTION = 4000;
/** A thumbnail larger than this is not copied. */
export const MAX_THUMB_BYTES = 2 * 1024 * 1024;

export type AssetType = "paints" | "bikes" | "liveries" | "kits" | "tracks" | "other";
export const ASSET_TYPES: AssetType[] = ["paints", "bikes", "liveries", "kits", "tracks", "other"];

// ───────────────────────────── classification ─────────────────────────────

/**
 * Which of our types a post is, from its categories and their ancestors.
 *
 * mxb-mods files a post under several categories at once ("Liveries", "KTM", "2021 KTM 450
 * SX-F OEM", "Beta 19"), so the first rule that any of them answers to wins, most specific
 * first. Ids rather than names: names are edited, ids are not. New subcategories are picked up
 * through the ancestor walk, which is why the tree is fetched rather than hard-coded.
 */
const TYPE_RULES: [AssetType, number[]][] = [
  // Tracks (and Beginner/Intermediate/Pro beneath it).
  ["tracks", [22]],
  // Bikes > Liveries.
  ["liveries", [37]],
  // Rider > Rider Kit.
  ["kits", [35]],
  // Rider gear and the smaller paintable parts: helmets, gloves, boots, protection, rider
  // and bike fonts, wheel textures.
  ["paints", [33, 32, 31, 36, 41, 42, 382]],
  // Everything else under Bikes (new bikes, model swaps, wheels, sounds) and Bike Packs.
  ["bikes", [29, 429]],
];
/** Manufacturers, and the per-model OEM categories: what `bike` is made of. */
const BIKE_ROOTS = [319, 147];

export interface Category {
  id: number;
  name: string;
  parent: number;
}

function ancestry(id: number, tree: Map<number, Category>): number[] {
  const out: number[] = [];
  let cur: number | undefined = id;
  for (let i = 0; cur && i < 12; i++) {
    out.push(cur);
    cur = tree.get(cur)?.parent;
  }
  return out;
}

export function classify(categoryIds: number[], tree: Map<number, Category>): AssetType {
  const lines = categoryIds.map((id) => ancestry(id, tree));
  for (const [type, roots] of TYPE_RULES) {
    if (lines.some((line) => line.some((id) => roots.includes(id)))) return type;
  }
  return "other";
}

/** The bike and manufacturer terms among a post's categories. */
export function bikeTerms(categoryIds: number[], tree: Map<number, Category>): string[] {
  const out: string[] = [];
  for (const id of categoryIds) {
    const line = ancestry(id, tree);
    if (line.slice(1).some((a) => BIKE_ROOTS.includes(a))) {
      const name = tree.get(id)?.name;
      if (name && !out.includes(name)) out.push(name);
    }
  }
  return out;
}

// ───────────────────────────── page parsing ─────────────────────────────

export interface DownloadOption {
  url: string;
  host: string;
  label: string;
  isDefault: boolean;
  isServer: boolean;
}

/** Does this text call something a server build? The word on its own, as the app reads it. */
export function mentionsServer(text: string): boolean {
  return /(?:^|[^a-z])servers?(?:[^a-z]|$)/i.test(text);
}

/** The file name a share URL ends in, stepping over MediaFire's `/file` and Drive's `/view`. */
export function urlFileName(url: string): string {
  const path = url.split(/[?#]/)[0].replace(/\/+$/, "");
  const segs = path.split("/").filter(Boolean);
  const last = segs.pop() ?? "";
  if (/^(file|view|download)$/i.test(last)) return segs.pop() ?? last;
  return last;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.toLowerCase().replace(/^www\./, "");
  } catch {
    return "";
  }
}

function stripTags(html: string): string {
  return decodeEntities(
    html
      .replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, " ")
      .replace(/<br\s*\/?>|<\/p>|<\/li>|<\/h\d>/gi, "\n")
      .replace(/<[^>]+>/g, " "),
  )
    .replace(/[ \t\f\v ]+/g, " ")
    .replace(/ *\n[ \n]*/g, "\n")
    .trim();
}

/**
 * The theme's `div.download-container` blocks, in page order. The app's `parse_downloads`,
 * on regexes because a Worker has no DOM: each block runs from its opening tag to the next
 * block or the instructions that follow the list.
 */
export function parseDownloads(html: string): DownloadOption[] {
  const out: DownloadOption[] = [];
  const opener = /<div\b[^>]*class="([^"]*\bdownload-container\b[^"]*)"[^>]*>/gi;
  const starts: { at: number; classes: string }[] = [];
  for (let m; (m = opener.exec(html)); ) starts.push({ at: m.index, classes: m[1] });
  for (let i = 0; i < starts.length; i++) {
    const end = i + 1 < starts.length ? starts[i + 1].at : html.indexOf('id="instructions"', starts[i].at);
    const block = html.slice(starts[i].at, end > starts[i].at ? end : starts[i].at + 4000);
    const href = /<a\b[^>]*\bhref="([^"]+)"/i.exec(block)?.[1];
    if (!href) continue;
    const url = decodeEntities(href.trim());
    if (!/^https?:\/\//i.test(url)) continue;
    const classes = starts[i].classes.toLowerCase();
    const label = stripTags(/<div\b[^>]*class="filename"[^>]*>([\s\S]*?)<\/div>/i.exec(block)?.[1] ?? "")
      .replace(/\s+/g, " ")
      .trim();
    const host = hostOf(url);
    const infoPage = host === "oem.mxb-mods.com" && label.toLowerCase().includes("information");
    out.push({
      url,
      host,
      label: label || host,
      isDefault:
        (classes.includes("container-default") || classes.includes("container-recommended")) && !infoPage,
      isServer: mentionsServer(stripTags(block)) || mentionsServer(urlFileName(url)),
    });
  }
  return out;
}

/** "Made for Beta 19" → "Beta 19", as the app normalises it. */
export function parseVersion(html: string): string | null {
  const m = /<p\b[^>]*class="betas"[^>]*>([\s\S]*?)<\/p>/i.exec(html);
  if (!m) return null;
  const text = stripTags(m[1]);
  const beta = /beta\s*[0-9]+(\.[0-9]+)*/i.exec(text);
  if (beta) return beta[0][0].toUpperCase() + beta[0].slice(1);
  return text || null;
}

/**
 * The byline above the title. Scoped to the post header: every comment names an author too.
 * Then the theme's `#authorName`, then the page's JSON-LD `Person`, then `author` meta.
 */
export function parseAuthor(html: string): string | null {
  const header = /<[a-z]+\b[^>]*class="[^"]*\bpost-header\b[^"]*"[\s\S]{0,4000}?<\/p>/i.exec(html)?.[0] ?? "";
  const linked = /<a\b[^>]*href="[^"]*\/author\/[^"]*"[^>]*>([\s\S]*?)<\/a>/i.exec(header);
  const byId = /<b\b[^>]*id="authorName"[^>]*>([\s\S]*?)<\/b>/i.exec(html);
  const ld = /"author":\s*\{[^{}]*?"@type":\s*"Person"[^{}]*?"name":\s*"([^"]{1,120})"/.exec(html);
  const meta = /<meta\s+name="author"\s+content="([^"]{1,120})"/i.exec(html);
  for (const raw of [linked?.[1], byId?.[1], ld?.[1], meta?.[1]]) {
    const name = raw ? stripTags(raw).trim() : "";
    if (name) return name.slice(0, 120);
  }
  return null;
}

/** The page's own picture: `og:image`, else the first image in the post body. */
export function parseImage(html: string): string | null {
  const og =
    /<meta\s+property="og:image"\s+content="([^"]+)"/i.exec(html)?.[1] ??
    /<meta\s+content="([^"]+)"\s+property="og:image"/i.exec(html)?.[1];
  const body = /<div\b[^>]*class="[^"]*\bentry-content\b[^"]*"[^>]*>([\s\S]*)/i.exec(html)?.[1] ?? "";
  const first = /<img\b[^>]*\bsrc="(https:\/\/[^"]+)"/i.exec(body)?.[1];
  for (const raw of [og, first]) {
    if (!raw) continue;
    const url = decodeEntities(raw.trim());
    if (/^https:\/\//i.test(url)) return url;
  }
  return null;
}

/** Whether a fetched page is Cloudflare's interstitial rather than the post. */
export function isChallenge(html: string): boolean {
  return /cf-browser-verification|challenge-platform|cf_chl_opt|Just a moment\.\.\./i.test(html);
}

// ───────────────────────────── robots.txt ─────────────────────────────

interface RobotsRule {
  allow: boolean;
  path: string;
}

/**
 * The rules robots.txt sets for us: the group naming our token if there is one, else `*`.
 * Longest match wins, Allow on a tie (RFC 9309).
 */
export function robotsRules(txt: string, token = ROBOTS_TOKEN): RobotsRule[] {
  const groups: { agents: string[]; rules: RobotsRule[] }[] = [];
  let cur: { agents: string[]; rules: RobotsRule[] } | null = null;
  let lastWasAgent = false;
  for (const raw of txt.split(/\r?\n/)) {
    const line = raw.replace(/#.*/, "").trim();
    const m = /^([a-z-]+)\s*:\s*(.*)$/i.exec(line);
    if (!m) continue;
    const field = m[1].toLowerCase();
    const value = m[2].trim();
    if (field === "user-agent") {
      if (!cur || !lastWasAgent) {
        cur = { agents: [], rules: [] };
        groups.push(cur);
      }
      cur.agents.push(value.toLowerCase());
      lastWasAgent = true;
      continue;
    }
    lastWasAgent = false;
    if (!cur) continue;
    if (field === "allow" || field === "disallow") {
      // An empty Disallow allows everything; it adds no rule.
      if (value) cur.rules.push({ allow: field === "allow", path: value });
    }
  }
  const mine = groups.filter((g) => g.agents.some((a) => a !== "*" && token.toLowerCase().includes(a)));
  const chosen = mine.length > 0 ? mine : groups.filter((g) => g.agents.includes("*"));
  return chosen.flatMap((g) => g.rules);
}

function ruleMatches(rule: string, path: string): number {
  const anchored = rule.endsWith("$");
  const body = anchored ? rule.slice(0, -1) : rule;
  const re = new RegExp(
    "^" + body.split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*") + (anchored ? "$" : ""),
  );
  return re.test(path) ? body.length : -1;
}

export function robotsAllows(rules: RobotsRule[], path: string): boolean {
  let best = -1;
  let allow = true;
  for (const r of rules) {
    const len = ruleMatches(r.path, path);
    if (len > best || (len === best && r.allow)) {
      if (len >= 0) {
        best = len;
        allow = r.allow;
      }
    }
  }
  return allow;
}

// ───────────────────────────── state ─────────────────────────────

async function getState<T>(env: Env, key: string): Promise<T | null> {
  const row = await env.DB.prepare("SELECT value FROM mirror_state WHERE key = ?").bind(key).first<{ value: string }>();
  if (!row) return null;
  try {
    return JSON.parse(row.value) as T;
  } catch {
    return null;
  }
}

async function setState(env: Env, key: string, value: unknown): Promise<void> {
  await env.DB.prepare(
    "INSERT INTO mirror_state (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
  )
    .bind(key, JSON.stringify(value))
    .run();
}

// ───────────────────────────── the polite client ─────────────────────────────

/**
 * The runtime's fetch, safe to store and pass around. Workers' `fetch` must be called with the
 * global scope as `this`; held in an object and called as its method, it throws "Illegal
 * invocation". A wrapper has no such requirement.
 */
export const globalFetch: typeof fetch = (input, init) => fetch(input, init);

export interface Sync {
  env: Env;
  now: number;
  fetch: typeof fetch;
  wait: (ms: number) => Promise<void>;
  rules: RobotsRule[];
  /** Requests made this run (or on this lane). */
  sent: number;
  /** Pictures copied this run (or on this lane). */
  images: number;
  /** Pause before each request after the first. */
  spacing: number;
  /** The time now, for budgets and cooldowns in a run that lasts minutes. */
  clock: () => number;
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** The site turned us away (403/429/503): the run stops and the next ones wait. */
class Refused extends Error {}

/**
 * One request: robots-checked, spaced, named. `null` when robots says no. From mxb-mods.com
 * or its CDN, a refusal puts the whole sync on a cooldown, because a site that has started
 * saying no to one request will say no to the next, and asking again is how an address gets
 * blocked. Another host (a picture on GitHub) only gets its answer back.
 */
async function modsGet(s: Sync, url: URL | string, accept = "application/json", spacing = s.spacing): Promise<Response | null> {
  const u = typeof url === "string" ? new URL(url, MODS_BASE) : url;
  const site = isModsHost(u.hostname);
  if (site && !robotsAllows(s.rules, u.pathname + u.search)) return null;
  if (s.sent > 0) await s.wait(spacing);
  s.sent++;
  // Called as a plain function, never as `s.fetch(…)`: the runtime's fetch throws "Illegal
  // invocation" when its `this` is anything but the global scope.
  const get = s.fetch;
  const res = await get(u.toString(), { headers: { "user-agent": UA, accept } });
  if (site && (res.status === 403 || res.status === 429 || res.status === 503)) {
    await res.body?.cancel().catch(() => {});
    await coolDown(s.env, s.clock(), res.status, res.headers.get("retry-after"));
    throw new Refused(`mxb-mods answered ${res.status}`);
  }
  return res;
}

/** Retry-After as milliseconds: seconds or an HTTP date. 0 when absent or nonsense. */
export function retryAfterMs(header: string | null, now: number): number {
  if (!header?.trim()) return 0;
  const secs = Number(header.trim());
  if (Number.isFinite(secs)) return Math.max(0, secs * 1000);
  const at = Date.parse(header);
  return Number.isFinite(at) ? Math.max(0, at - now) : 0;
}

/** How long a refusal keeps the sync away: 10 min, 20, 40 … 2 h, or Retry-After if longer. */
export function cooldownMs(strikes: number, retryAfter: number): number {
  return Math.min(DAY, Math.max(Math.min(COOLDOWN_MIN_MS * 2 ** strikes, COOLDOWN_MAX_MS), retryAfter));
}

interface Cooldown {
  until: number;
  status: number;
  strikes?: number;
}

async function coolDown(env: Env, now: number, status: number, retryAfter: string | null): Promise<void> {
  const prev = await getState<Cooldown>(env, "cooldown");
  // Several lanes refused at once are one refusal.
  if (prev && prev.until > now) return;
  // Refused again within the hour after the last cooldown ended: twice as long.
  const strikes = prev && prev.until > now - HOUR ? Math.min((prev.strikes ?? 0) + 1, 8) : 0;
  const wait = cooldownMs(strikes, retryAfterMs(retryAfter, now));
  await setState(env, "cooldown", { until: now + wait, status, strikes } satisfies Cooldown);
  console.warn(JSON.stringify({ msg: "mirror cooling down", status, minutes: Math.round(wait / MINUTE), strikes }));
}

async function coolingUntil(env: Env, now: number): Promise<number> {
  const c = await getState<Cooldown>(env, "cooldown");
  return c && c.until > now ? c.until : 0;
}

async function loadRobots(env: Env, now: number, f: typeof fetch): Promise<RobotsRule[]> {
  const cached = await getState<{ at: number; txt: string }>(env, "robots");
  if (cached && now - cached.at < ROBOTS_TTL_MS) return robotsRules(cached.txt);
  try {
    const res = await f(`${MODS_BASE}/robots.txt`, { headers: { "user-agent": UA } });
    // A missing robots.txt allows everything; an unreachable one keeps what we had, or, the
    // first time, disallows everything (RFC 9309 treats 5xx that way).
    if (res.status >= 500) return cached ? robotsRules(cached.txt) : [{ allow: false, path: "/" }];
    const txt = res.ok ? await res.text() : "";
    await setState(env, "robots", { at: now, txt });
    return robotsRules(txt);
  } catch {
    return cached ? robotsRules(cached.txt) : [{ allow: false, path: "/" }];
  }
}

async function loadCategories(s: Sync): Promise<Map<number, Category>> {
  const cached = await getState<{ at: number; cats: Category[] }>(s.env, "categories");
  if (cached && s.now - cached.at < CATEGORY_TTL_MS) return new Map(cached.cats.map((c) => [c.id, c]));
  const cats: Category[] = [];
  for (let page = 1; page <= 10; page++) {
    const u = new URL("/wp-json/wp/v2/categories", MODS_BASE);
    u.searchParams.set("per_page", "100");
    u.searchParams.set("page", String(page));
    u.searchParams.set("_fields", "id,name,parent");
    const res = await modsGet(s, u);
    if (!res || !res.ok) break;
    const list = (await res.json()) as { id: number; name: string; parent: number }[];
    for (const c of list) cats.push({ id: c.id, name: decodeEntities(c.name), parent: c.parent });
    if (list.length < 100) break;
  }
  if (cats.length === 0) return new Map((cached?.cats ?? []).map((c) => [c.id, c]));
  await setState(s.env, "categories", { at: s.now, cats });
  return new Map(cats.map((c) => [c.id, c]));
}

// ───────────────────────────── 1. discover ─────────────────────────────

export interface Post {
  id: number;
  slug: string;
  link: string;
  modified: string;
  title?: { rendered?: string };
  content?: { rendered?: string };
  categories?: number[];
  _embedded?: Record<string, any>;
}

/**
 * Where the listing walk stands. mxb-mods.com ignores `order` and `orderby` and always answers
 * newest-modified first, so the walk doesn't rely on any order: it pages through everything
 * modified since `hwm` (the newest `modified` a finished walk saw) and only moves `hwm` once
 * the walk reaches the end. The first walk has no `hwm` and covers the whole catalogue.
 */
interface Listing {
  hwm: string;
  walk: { top: string; offset: number } | null;
}
/** Pages overlap by this many posts, so an edit that shifts the list mid-walk skips nothing. */
const LIST_OVERLAP = 5;

/** The post's picture at tile size, not the full upload. */
export function thumbOf(p: Post): string | null {
  const media = p._embedded?.["wp:featuredmedia"]?.[0];
  for (const size of ["medium_large", "medium", "large", "full"]) {
    const src = media?.media_details?.sizes?.[size]?.source_url;
    if (typeof src === "string" && src.startsWith("https://")) return src;
  }
  return typeof media?.source_url === "string" && media.source_url.startsWith("https://") ? media.source_url : null;
}

/** The post's author from the REST listing's embedded user. */
export function restAuthor(p: Post): string | null {
  const name = p._embedded?.author?.[0]?.name;
  return typeof name === "string" && name.trim() ? decodeEntities(name).trim().slice(0, 120) : null;
}

/**
 * Upsert one post from the listing as a mirrored asset. Its page is re-read only when the
 * source says it changed. Moderation (`state`) is never touched here: a hidden mod stays hidden
 * however often the source edits it.
 */
export async function upsertPost(env: Env, p: Post, tree: Map<number, Category>, now: number): Promise<void> {
  const cats = p.categories ?? [];
  const title = decodeEntities(String(p.title?.rendered ?? "")).trim() || p.slug;
  const description = stripTags(String(p.content?.rendered ?? "")).slice(0, MAX_DESCRIPTION);
  const names = cats.map((c) => tree.get(c)?.name).filter((n): n is string => !!n);
  await env.DB.prepare(
    `INSERT INTO mod_assets (source, source_ref, slug, title, type, bike, categories, description, thumb_src,
       source_url, modified, first_seen, last_seen, page_status, page_due_at, public_id, author)
     VALUES ('mirror', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, 'due', 0, ?12, ?13)
     ON CONFLICT (source_ref) DO UPDATE SET
       slug = excluded.slug, title = excluded.title, type = excluded.type, bike = excluded.bike,
       categories = excluded.categories, description = excluded.description,
       author = COALESCE(mod_assets.author, excluded.author),
       thumb_src = COALESCE(excluded.thumb_src, mod_assets.thumb_src), source_url = excluded.source_url,
       last_seen = excluded.last_seen,
       page_status = CASE WHEN mod_assets.modified = excluded.modified AND mod_assets.page_status <> 'gone'
                          THEN mod_assets.page_status ELSE 'due' END,
       page_attempts = CASE WHEN mod_assets.modified = excluded.modified THEN mod_assets.page_attempts ELSE 0 END,
       page_due_at = CASE WHEN mod_assets.modified = excluded.modified THEN mod_assets.page_due_at ELSE 0 END,
       modified = excluded.modified`,
  )
    .bind(
      p.id,
      p.slug,
      title,
      classify(cats, tree),
      bikeTerms(cats, tree).join("; "),
      names.join("; "),
      description,
      thumbOf(p),
      p.link,
      p.modified,
      now,
      newPublicId(),
      restAuthor(p),
    )
    .run();
}

/**
 * The listing walk, to its end in one run when it can. Every page is saved as it lands, so a
 * run that reaches its time budget leaves the next one exactly where it stopped.
 */
async function discover(s: Sync, tree: Map<number, Category>): Promise<void> {
  const st = (await getState<Listing>(s.env, "listing")) ?? { hwm: "", walk: null };
  const started = s.clock();
  for (let n = 0; n < LIST_MAX_PAGES_PER_RUN && s.clock() - started < DISCOVER_BUDGET_MS; n++) {
    const walk = st.walk ?? { top: "", offset: 0 };
    st.walk = walk;
    const u = new URL("/wp-json/wp/v2/posts", MODS_BASE);
    u.searchParams.set("orderby", "modified");
    u.searchParams.set("order", "desc");
    u.searchParams.set("per_page", String(LIST_PER_PAGE));
    if (walk.offset > 0) u.searchParams.set("offset", String(walk.offset));
    u.searchParams.set("_embed", "wp:featuredmedia,author");
    u.searchParams.set("_fields", "id,slug,link,modified,title,content,categories,_links,_embedded");
    // One second back, so posts sharing the high-water second are not skipped.
    if (st.hwm) u.searchParams.set("modified_after", minusOneSecond(st.hwm));
    const res = await modsGet(s, u);
    if (!res) return;
    // WordPress answers an offset past the end with 400: the walk is done.
    const posts = res.status === 400 ? [] : res.ok ? ((await res.json()) as Post[]) : null;
    if (!posts) throw new Error(`listing answered ${res.status}`);
    for (const p of posts) {
      await upsertPost(s.env, p, tree, s.now);
      if (p.modified > walk.top) walk.top = p.modified;
    }
    if (posts.length < LIST_PER_PAGE) {
      if (walk.top > st.hwm) st.hwm = walk.top;
      st.walk = null;
      await setState(s.env, "listing", st);
      return;
    }
    walk.offset += LIST_PER_PAGE - LIST_OVERLAP;
    await setState(s.env, "listing", st);
  }
}

export function minusOneSecond(local: string): string {
  const t = Date.parse(local + "Z");
  if (!Number.isFinite(t)) return local;
  return new Date(t - 1000).toISOString().slice(0, 19);
}

/** Walk every post id, a page a run, so `last_seen` means something and deletions surface. */
async function sweep(s: Sync): Promise<void> {
  const st = (await getState<{ page: number; started: number; lastComplete: number }>(s.env, "sweep")) ?? {
    page: 1,
    started: s.now,
    lastComplete: 0,
  };
  for (let n = 0; n < SWEEP_PAGES_PER_RUN; n++) {
    const u = new URL("/wp-json/wp/v2/posts", MODS_BASE);
    u.searchParams.set("orderby", "id");
    u.searchParams.set("order", "asc");
    u.searchParams.set("per_page", "100");
    u.searchParams.set("page", String(st.page));
    u.searchParams.set("_fields", "id");
    const res = await modsGet(s, u);
    if (!res) return;
    const ids = res.ok ? ((await res.json()) as { id: number }[]).map((p) => p.id) : [];
    if (res.ok && ids.length > 0) {
      // One JSON parameter, not one per id: D1 refuses a statement with more than 100 bound
      // parameters, and a page is 100 ids plus the time. That refusal used to stop every run
      // here, before a single page was read.
      await s.env.DB.prepare(
        `UPDATE mod_assets SET last_seen = ? WHERE source = 'mirror' AND source_ref IN (SELECT value FROM json_each(?))`,
      )
        .bind(s.now, JSON.stringify(ids))
        .run();
    }
    if (res.status === 400 || (res.ok && ids.length < 100)) {
      // A whole sweep done: anything it didn't see, and nothing has seen for a fortnight, is gone.
      await s.env.DB.prepare(
        `UPDATE mod_assets SET page_status = 'gone'
         WHERE source = 'mirror' AND last_seen < ? AND last_seen < ? AND page_status <> 'gone'`,
      )
        .bind(st.started, s.now - GONE_AFTER_MS)
        .run();
      await setState(s.env, "sweep", { page: 1, started: s.now, lastComplete: s.now });
      return;
    }
    if (!res.ok) return;
    st.page++;
    await setState(s.env, "sweep", st);
  }
}

// ───────────────────────────── 2. read pages ─────────────────────────────

export interface DueAsset {
  id: number;
  source_url: string;
  thumb_src: string | null;
  thumb_key: string | null;
  page_attempts: number;
}

export const DUE_COLUMNS = "id, source_url, thumb_src, thumb_key, page_attempts";

/** With no queue to hand them to (a local run), the cron reads a few pages itself. */
async function readPages(s: Sync): Promise<void> {
  // The fetcher leases pages itself (`mirrorfetcher.ts`).
  if (pagesViaFetcher(s.env)) return;
  const { results } = await s.env.DB.prepare(
    `SELECT ${DUE_COLUMNS} FROM mod_assets
     WHERE source = 'mirror' AND page_status IN ('due', 'retry') AND page_due_at <= ?
     ORDER BY page_due_at, id LIMIT ?`,
  )
    .bind(s.now, PAGES_PER_RUN)
    .all<DueAsset>();
  for (const asset of results) {
    try {
      await readPage(s, asset);
    } catch (err) {
      if (err instanceof Refused) throw err;
      await pageRetry(s, asset, String(err));
    }
  }
}

/**
 * Lease the pages that are due (new and changed posts first, then rows an older parser read)
 * and hand them to the queue, one message each, until `target` are out. A lease that runs out
 * (its message lost) is sent again.
 */
export async function dispatchPages(env: Env, now: number, target = PAGE_QUEUE_TARGET): Promise<number> {
  // With the pages going via the fetcher, it leases due rows itself: nothing to queue.
  if (!env.MIRROR_QUEUE || pagesViaFetcher(env)) return 0;
  const out = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM mod_assets WHERE source = 'mirror' AND page_status = 'queued' AND page_due_at > ?",
  )
    .bind(now)
    .first<{ n: number }>();
  const room = target - (out?.n ?? 0);
  if (room <= 0) return 0;
  const { results } = await env.DB.prepare(
    `SELECT id FROM mod_assets
     WHERE source = 'mirror' AND (
       (page_status IN ('due', 'retry', 'queued') AND page_due_at <= ?1)
       OR (page_status = 'ok' AND page_rev < ?2))
     ORDER BY CASE WHEN page_status = 'ok' THEN 1 ELSE 0 END, page_due_at, id DESC
     LIMIT ?3`,
  )
    .bind(now, PAGE_REV, room)
    .all<{ id: number }>();
  if (results.length === 0) return 0;
  const ids = results.map((r) => r.id);
  // One JSON parameter for the ids: D1 refuses a statement with more than 100 bound parameters.
  await env.DB.prepare(
    "UPDATE mod_assets SET page_status = 'queued', page_due_at = ? WHERE id IN (SELECT value FROM json_each(?))",
  )
    .bind(now + PAGE_LEASE_MS, JSON.stringify(ids))
    .run();
  // A queue takes at most 100 messages a send.
  for (let i = 0; i < ids.length; i += 100) {
    await env.MIRROR_QUEUE.sendBatch(ids.slice(i, i + 100).map((id) => ({ body: { kind: "page", id } satisfies MirrorJob })));
  }
  return ids.length;
}

export interface PageBatchResult {
  read: number;
  images: number;
  requests: number;
  /** Put back for later: a cooldown began, or the sync is switched off. */
  deferred: number;
}

/**
 * The queue consumer's half of a page read: the batch's leased rows, read on `PAGE_LANES`
 * lanes side by side, each lane spacing its own requests. A refusal (or a cooldown another
 * invocation started) stops every lane; what is left goes back to `due` for after it.
 */
export async function readPageJobs(env: Env, ids: number[], opts: RunOptions = {}): Promise<PageBatchResult> {
  const clock = opts.clock ?? Date.now;
  const now = opts.now ?? clock();
  const f = opts.fetch ?? globalFetch;
  const wait = opts.wait ?? sleep;
  const result: PageBatchResult = { read: 0, images: 0, requests: 0, deferred: 0 };
  if (ids.length === 0) return result;
  const { results } = await env.DB.prepare(
    `SELECT ${DUE_COLUMNS} FROM mod_assets
     WHERE source = 'mirror' AND page_status = 'queued' AND id IN (SELECT value FROM json_each(?))`,
  )
    .bind(JSON.stringify(ids))
    .all<DueAsset>();
  const todo = [...results];
  const back: number[] = [];
  // Switched off, or the pages now go via the fetcher (a message queued before the switch):
  // straight back to `due`, where the fetcher's lease finds them.
  let until = (env.MXB_MIRROR ?? "off") !== "on" || pagesViaFetcher(env) ? now : await coolingUntil(env, now);
  const rules = until ? [] : await loadRobots(env, now, f);

  const lane = async (k: number) => {
    const s: Sync = { env, now, clock, fetch: f, wait, rules, sent: 0, images: 0, spacing: PAGE_SPACING_MS };
    // A lane's first request waits too: a batch is one message, so without this two
    // invocations back to back would reach the site with no gap. Lanes also start apart.
    if (!until) await wait(PAGE_SPACING_MS + Math.round((k * PAGE_SPACING_MS) / PAGE_LANES));
    let asset: DueAsset | undefined;
    while (!until && (asset = todo.shift())) {
      const cooling = await coolingUntil(env, clock());
      if (cooling) {
        until = cooling;
        back.push(asset.id);
        break;
      }
      try {
        await readPage(s, asset);
        result.read++;
      } catch (err) {
        if (err instanceof Refused) {
          until = (await coolingUntil(env, clock())) || clock();
          back.push(asset.id);
          break;
        }
        await pageRetry(s, asset, String(err));
      }
    }
    result.requests += s.sent;
    result.images += s.images;
  };
  await Promise.all(Array.from({ length: Math.min(PAGE_LANES, todo.length) }, (_, k) => lane(k)));

  back.push(...todo.map((a) => a.id));
  if (back.length) {
    await env.DB.prepare(
      "UPDATE mod_assets SET page_status = 'due', page_due_at = ? WHERE page_status = 'queued' AND id IN (SELECT value FROM json_each(?))",
    )
      .bind(until, JSON.stringify(back))
      .run();
  }
  result.deferred = back.length;
  return result;
}

async function readPage(s: Sync, asset: DueAsset): Promise<void> {
  const url = new URL(asset.source_url);
  if (url.hostname !== "mxb-mods.com") {
    await s.env.DB.prepare("UPDATE mod_assets SET page_status = 'gone', page_error = 'off-site link' WHERE id = ?")
      .bind(asset.id)
      .run();
    return;
  }
  const res = await modsGet(s, url, "text/html,application/xhtml+xml");
  if (!res) {
    await s.env.DB.prepare("UPDATE mod_assets SET page_status = 'robots' WHERE id = ?").bind(asset.id).run();
    return;
  }
  if (res.status === 404 || res.status === 410) {
    await s.env.DB.prepare("UPDATE mod_assets SET page_status = 'gone' WHERE id = ?").bind(asset.id).run();
    return;
  }
  if (!res.ok) return pageRetry(s, asset, `page answered ${res.status}`);
  const page = parsePage(await res.text(), asset);
  if (!page) return pageRetry(s, asset, "challenge page");
  // The links first: they are what the page is for, and nothing after may lose them.
  await writeMirrorVersion(s.env, asset.id, page.version, page.downloads, s.now);
  await finishPage(s.env, asset, page, { thumb: (src) => copyThumb(s, src), image: (src) => copyImage(s, src) }, s.clock());
}

// ───────────────────────────── a page, from either source ─────────────────────────────

/**
 * What a post page offers, parsed. The HTML comes from the Worker's own read (`readPage`) or
 * from the fetcher on our own box (`mirrorfetcher.ts`); both go through this and the two
 * writers below, so a page reads the same whichever way it arrived.
 */
export interface ParsedPage {
  downloads: DownloadOption[];
  version: string | null;
  author: string | null;
  /** The thumbnail's source: the listing's, else the page's own picture. */
  src: string | null;
  body: ReturnType<typeof parsePostBody>;
}

/** The page, or null when it is Cloudflare's interstitial rather than the post. */
export function parsePage(html: string, asset: Pick<DueAsset, "thumb_src">): ParsedPage | null {
  const downloads = parseDownloads(html);
  if (downloads.length === 0 && isChallenge(html)) return null;
  return {
    downloads,
    version: parseVersion(html),
    author: parseAuthor(html),
    src: asset.thumb_src ?? parseImage(html),
    body: parsePostBody(html),
  };
}

/** Where a page's pictures come from: copied by the Worker, or uploaded by the fetcher. */
export interface ImageSource {
  /** The thumbnail, stored; null when it can't be had. */
  thumb(src: string | null): Promise<Thumb | null>;
  /** One content image's SHA-256, stored; null when it can't be had. */
  image(src: string): Promise<string | null>;
}

/** The page's pictures, words and byline, after its links are written: the row is read. */
export async function finishPage(
  env: Env,
  asset: Pick<DueAsset, "id" | "thumb_key">,
  page: Omit<ParsedPage, "downloads" | "version">,
  images: ImageSource,
  readAt: number,
): Promise<void> {
  const thumb = asset.thumb_key ? null : await images.thumb(page.src);
  await copyImages(env, asset.id, page.body.images, images.image);
  await env.DB.prepare(
    `UPDATE mod_assets SET author = COALESCE(?, author), thumb_src = COALESCE(thumb_src, ?),
       thumb_sha = COALESCE(?, thumb_sha), thumb_key = COALESCE(?, thumb_key), body = ?,
       page_status = 'ok', page_attempts = 0, page_error = NULL, page_rev = ?, page_read_at = ? WHERE id = ?`,
  )
    .bind(
      page.author,
      page.src,
      thumb?.sha ?? null,
      thumb?.key ?? null,
      page.body.blocks.length ? JSON.stringify(page.body.blocks) : null,
      PAGE_REV,
      readAt,
      asset.id,
    )
    .run();
}

/** The content images a post already holds, by source. */
export async function heldImages(env: Env, assetId: number): Promise<Map<string, string>> {
  const { results } = await env.DB.prepare("SELECT src, sha256 FROM mod_asset_images WHERE asset_id = ?")
    .bind(assetId)
    .all<{ src: string; sha256: string }>();
  return new Map(results.map((r) => [r.src, r.sha256]));
}

/** A thumbnail some post already stored from this source. */
export async function heldThumb(env: Env, src: string): Promise<Thumb | null> {
  return await env.DB.prepare(
    "SELECT thumb_sha AS sha, thumb_key AS key FROM mod_assets WHERE thumb_src = ? AND thumb_key IS NOT NULL LIMIT 1",
  )
    .bind(src)
    .first<Thumb>();
}

/**
 * The post's content images into the public bucket as `img/<sha256>.<ext>`, in page order, at
 * most 12 and each at most 3 MB. A picture the post already has (same source) is not fetched
 * again. The new list replaces the old one.
 */
async function copyImages(
  env: Env,
  assetId: number,
  wanted: PostImage[],
  copy: (src: string) => Promise<string | null>,
): Promise<void> {
  if (!env.ASSET_MIRROR) return;
  const known = await heldImages(env, assetId);
  const rows: (PostImage & { sha: string })[] = [];
  for (const img of wanted.slice(0, MAX_IMAGES)) {
    const sha = known.get(img.src) ?? (await copy(img.src));
    if (sha && !rows.some((r) => r.sha === sha)) rows.push({ ...img, sha });
  }
  if (known.size === 0 && rows.length === 0) return;
  // A statement per picture, six parameters each: far under D1's 100 a statement.
  await env.DB.batch([
    env.DB.prepare("DELETE FROM mod_asset_images WHERE asset_id = ?").bind(assetId),
    ...rows.map((r, idx) =>
      env.DB.prepare(
        "INSERT INTO mod_asset_images (asset_id, idx, sha256, src, width, height) VALUES (?, ?, ?, ?, ?, ?)",
      ).bind(assetId, idx, r.sha, r.src.slice(0, 1000), r.width, r.height),
    ),
  ]);
}

/** One picture: a real image by its first bytes (never SVG), under the cap, stored by SHA-256. */
async function copyImage(s: Sync, src: string): Promise<string | null> {
  try {
    const res = await modsGet(s, new URL(src), "image/avif,image/webp,image/png,image/jpeg,image/gif", IMAGE_SPACING_MS);
    if (!res) return null;
    if (!res.ok || Number(res.headers.get("content-length") ?? 0) > MAX_IMAGE_BYTES) {
      await res.body?.cancel().catch(() => {});
      return null;
    }
    const bytes = await readCapped(res, MAX_IMAGE_BYTES);
    const type = bytes && bytes.byteLength > 0 ? sniffImage(bytes) : null;
    if (!bytes || !type) return null;
    const ext = thumbExt(type) ?? "bin";
    s.images++;
    const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
    return await putSmallBlob(s.env, buf, type, urlFileName(src), s.now, (sha) => `img/${sha}.${ext}`);
  } catch (err) {
    if (err instanceof Refused) throw err;
    return null;
  }
}

/** A response body, or null once it passes `cap` bytes (the rest is never downloaded). */
export async function readCapped(res: Response, cap: number): Promise<Uint8Array | null> {
  if (!res.body) return null;
  const reader = res.body.getReader();
  const parts: Uint8Array[] = [];
  let n = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    n += value.byteLength;
    if (n > cap) {
      await reader.cancel().catch(() => {});
      return null;
    }
    parts.push(value);
  }
  const out = new Uint8Array(n);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.byteLength;
  }
  return out;
}

function pageRetry(s: Sync, asset: DueAsset, error: string): Promise<void> {
  return pageFailed(s.env, s.now, asset, error);
}

/** A page read that failed: retried with backoff, given up on after `PAGE_MAX_ATTEMPTS`. */
export async function pageFailed(env: Env, now: number, asset: Pick<DueAsset, "id" | "page_attempts">, error: string): Promise<void> {
  const attempts = asset.page_attempts + 1;
  const status = attempts >= PAGE_MAX_ATTEMPTS ? "gone" : "retry";
  await env.DB.prepare(
    "UPDATE mod_assets SET page_status = ?, page_attempts = ?, page_due_at = ?, page_error = ? WHERE id = ?",
  )
    .bind(status, attempts, now + backoff(attempts), error.slice(0, 300), asset.id)
    .run();
}

/** 10 min, 20, 40 … capped at a day, with a little jitter so retries don't arrive together. */
export function backoff(attempts: number, rand = Math.random): number {
  const base = Math.min(10 * MINUTE * 2 ** Math.max(0, attempts - 1), DAY);
  return Math.round(base * (0.9 + rand() * 0.2));
}

/**
 * Record what the page offers now. The same links as the current version only refresh its
 * labels; a different list is a new version, and every link the old version had already
 * mirrored (or listed out of a folder) carries its state across, so nothing is fetched twice.
 */
export async function writeMirrorVersion(
  env: Env,
  assetId: number,
  label: string | null,
  downloads: DownloadOption[],
  now: number,
): Promise<number> {
  const current = await env.DB.prepare(
    "SELECT v.id, v.seq FROM mod_assets a JOIN mod_versions v ON v.id = a.current_version WHERE a.id = ?",
  )
    .bind(assetId)
    .first<{ id: number; seq: number }>();
  const prevUrls = current
    ? (
        await env.DB.prepare("SELECT url FROM mod_files WHERE version_id = ? AND part = 0 ORDER BY idx")
          .bind(current.id)
          .all<{ url: string }>()
      ).results.map((r) => r.url)
    : null;
  const urls = downloads.map((d) => d.url);

  if (current && prevUrls && prevUrls.length === urls.length && prevUrls.every((u, i) => u === urls[i])) {
    await env.DB.batch([
      env.DB.prepare("UPDATE mod_versions SET label = ? WHERE id = ?").bind(label, current.id),
      ...downloads.map((d, idx) =>
        env.DB.prepare(
          "UPDATE mod_files SET label = ?, host = ?, is_server = ?, is_default = ? WHERE version_id = ? AND idx = ?",
        ).bind(d.label.slice(0, 200), d.host, d.isServer ? 1 : 0, d.isDefault ? 1 : 0, current.id, idx),
      ),
    ]);
    return current.id;
  }

  const seq = (current?.seq ?? 0) + 1;
  const created = await env.DB.prepare(
    "INSERT INTO mod_versions (asset_id, seq, label, state, created_at, created_by) VALUES (?, ?, ?, 'live', ?, 'mirror') RETURNING id",
  )
    .bind(assetId, seq, label, now)
    .first<{ id: number }>();
  const vid = created!.id;
  const stmts: D1PreparedStatement[] = [];
  downloads.forEach((d, idx) => {
    if (current) {
      // Carry the old version's row for this link, and its folder parts, across.
      stmts.push(
        env.DB.prepare(
          `INSERT INTO mod_files (version_id, idx, part, rel, url, host, label, is_server, is_default, status,
             attempts, due_at, sha256, filename, error, fetched_at)
           SELECT ?1, ?2, p.part, p.rel, p.url, p.host, CASE WHEN p.part = 0 THEN ?3 ELSE p.label END, ?4, ?5,
             CASE WHEN p.status = 'queued' THEN 'pending' ELSE p.status END,
             p.attempts, p.due_at, p.sha256, p.filename, p.error, p.fetched_at
           FROM mod_files p
           WHERE p.version_id = ?6 AND p.idx = (
             SELECT idx FROM mod_files WHERE version_id = ?6 AND part = 0 AND url = ?7 ORDER BY idx LIMIT 1)`,
        ).bind(vid, idx, d.label.slice(0, 200), d.isServer ? 1 : 0, d.isDefault ? 1 : 0, current.id, d.url),
      );
    }
    stmts.push(
      env.DB.prepare(
        `INSERT OR IGNORE INTO mod_files (version_id, idx, part, url, host, label, is_server, is_default, status)
         VALUES (?, ?, 0, ?, ?, ?, ?, ?, 'idle')`,
      ).bind(vid, idx, d.url, d.host, d.label.slice(0, 200), d.isServer ? 1 : 0, d.isDefault ? 1 : 0),
    );
  });
  stmts.push(env.DB.prepare("UPDATE mod_assets SET current_version = ? WHERE id = ?").bind(vid, assetId));
  await env.DB.batch(stmts);
  return vid;
}

/**
 * The featured image, copied into the public bucket as `thumbs/<sha256>.<ext>`. A Worker has no
 * image library, so it isn't resized here: the sync asks WordPress for its 768-wide size
 * (`thumbOf`) and anything over the cap is skipped.
 */
async function copyThumb(s: Sync, src: string | null): Promise<Thumb | null> {
  if (!src || !s.env.ASSET_MIRROR) return null;
  const known = await heldThumb(s.env, src);
  if (known) return known;
  try {
    const u = new URL(src);
    if (u.hostname !== "mxb-mods.com") return null;
    const res = await modsGet(s, u, "image/*", IMAGE_SPACING_MS);
    const type = (res?.headers.get("content-type") ?? "").split(";")[0].trim().toLowerCase();
    if (!res || !res.ok || !thumbExt(type)) return null;
    const bytes = await res.arrayBuffer();
    if (bytes.byteLength === 0 || bytes.byteLength > MAX_THUMB_BYTES) return null;
    return await putThumb(s.env, bytes, type, urlFileName(src), s.now);
  } catch (err) {
    if (err instanceof Refused) throw err;
    return null;
  }
}

export interface Thumb {
  sha: string;
  key: string;
}

/** The picture types a thumbnail may be, and the extension each is stored under. */
const THUMB_TYPES: Record<string, string> = {
  "image/jpeg": "jpg",
  "image/png": "png",
  "image/webp": "webp",
  "image/gif": "gif",
  "image/avif": "avif",
};
export function thumbExt(type: string): string | null {
  return THUMB_TYPES[type] ?? null;
}

/** Does the start of the file match the type it claims? A renamed file is not a picture. */
export function sniffImage(bytes: Uint8Array): string | null {
  const b = bytes;
  const at = (i: number, s: string) => [...s].every((c, j) => b[i + j] === c.charCodeAt(0));
  if (b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return "image/jpeg";
  if (b[0] === 0x89 && at(1, "PNG")) return "image/png";
  if (at(0, "RIFF") && at(8, "WEBP")) return "image/webp";
  if (at(0, "GIF8")) return "image/gif";
  if (at(4, "ftypavif") || at(4, "ftypavis")) return "image/avif";
  return null;
}

/** A picture into the public bucket as `thumbs/<sha256>.<ext>`, once per distinct file. */
export async function putThumb(env: Env, bytes: ArrayBuffer, type: string, filename: string, now: number): Promise<Thumb> {
  const ext = thumbExt(type) ?? "bin";
  const sha = await putSmallBlob(env, bytes, type, filename, now, (s) => `thumbs/${s}.${ext}`);
  // The same bytes stored earlier keep the key they were stored under.
  const row = await env.DB.prepare("SELECT r2_key FROM mod_blobs WHERE sha256 = ?").bind(sha).first<{ r2_key: string }>();
  return { sha, key: row?.r2_key ?? `thumbs/${sha}.${ext}` };
}

/** A small buffer (a picture) into the public bucket, by default under `other/<sha256>`. */
export async function putSmallBlob(
  env: Env,
  bytes: ArrayBuffer,
  type: string,
  filename: string,
  now: number,
  keyOf: (sha: string) => string = (sha) => `other/${sha}`,
): Promise<string> {
  const sha = hex(await crypto.subtle.digest("SHA-256", bytes));
  const key = keyOf(sha);
  const have = await env.DB.prepare("SELECT 1 FROM mod_blobs WHERE sha256 = ?").bind(sha).first();
  if (!have) {
    await env.ASSET_MIRROR.put(key, bytes, {
      httpMetadata: { contentType: type, cacheControl: "public, max-age=31536000, immutable" },
    });
    await env.DB.prepare(
      `INSERT OR IGNORE INTO mod_blobs (sha256, bucket, r2_key, size, content_type, filename, first_seen)
       VALUES (?, 'public', ?, ?, ?, ?, ?)`,
    )
      .bind(sha, key, bytes.byteLength, type, filename, now)
      .run();
  }
  return sha;
}

export function hex(buf: ArrayBuffer): string {
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// ───────────────────────────── 3. dispatch ─────────────────────────────

/** A queue message: one mirrored file to fetch, one upload to verify, or one post page to read. */
export type MirrorJob =
  | { kind: "file"; version: number; idx: number; part: number }
  | { kind: "upload"; id: string }
  | { kind: "page"; id: number };

/**
 * Lease the current versions' files that are due and hand them to the queue. A file whose host
 * goes via the fetcher (`fetcherroute.ts`) is marked `fetcher` instead, for its lease to find.
 */
export async function dispatch(env: Env, now: number, limit = DISPATCH_PER_RUN): Promise<number> {
  if (!env.MIRROR_QUEUE) return 0;
  const { results } = await env.DB.prepare(
    `SELECT f.version_id, f.idx, f.part, f.url FROM mod_files f
     JOIN mod_assets a ON a.current_version = f.version_id
     WHERE a.source = 'mirror' AND a.page_status = 'ok' AND a.state = 'active' AND f.url IS NOT NULL AND (
       (f.status IN ('pending', 'retry') AND f.due_at <= ?1) OR (f.status = 'queued' AND f.leased_until < ?1))
     ORDER BY f.due_at, a.id DESC, f.idx, f.part LIMIT ?2`,
  )
    .bind(now, limit)
    .all<{ version_id: number; idx: number; part: number; url: string }>();
  if (results.length === 0) return 0;
  const viaFetcher = await fetcherRouter(env);
  const fetcher = results.filter((r) => viaFetcher(r.url));
  const queued = results.filter((r) => !viaFetcher(r.url));
  await env.DB.batch([
    ...queued.map((r) =>
      env.DB.prepare(
        "UPDATE mod_files SET status = 'queued', leased_until = ? WHERE version_id = ? AND idx = ? AND part = ?",
      ).bind(now + LEASE_MS, r.version_id, r.idx, r.part),
    ),
    ...fetcher.map((r) =>
      env.DB.prepare(
        "UPDATE mod_files SET status = 'fetcher', due_at = ?, leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?",
      ).bind(now, r.version_id, r.idx, r.part),
    ),
  ]);
  if (queued.length) {
    await env.MIRROR_QUEUE.sendBatch(
      queued.map((r) => ({ body: { kind: "file", version: r.version_id, idx: r.idx, part: r.part } satisfies MirrorJob })),
    );
  }
  return results.length;
}

// ───────────────────────────── the run ─────────────────────────────

export interface RunOptions {
  now?: number;
  fetch?: typeof fetch;
  wait?: (ms: number) => Promise<void>;
  /** The time now, as the run goes on. `Date.now` unless a test holds the clock. */
  clock?: () => number;
}

/**
 * The backfill's rate, in the log: pages read in the last ten minutes and hour, what is left
 * (due, queued, and rows an older parser read), and when that runs out at this rate.
 */
async function logBackfill(env: Env, now: number, dispatched: number): Promise<void> {
  const r = await env.DB.prepare(
    `SELECT
       (SELECT COUNT(*) FROM mod_assets WHERE page_read_at > ?1) AS last10,
       (SELECT COUNT(*) FROM mod_assets WHERE page_read_at > ?2) AS last60,
       (SELECT COUNT(*) FROM mod_assets WHERE source = 'mirror'
          AND (page_status IN ('due', 'retry', 'queued') OR (page_status = 'ok' AND page_rev < ?3))) AS remaining`,
  )
    .bind(now - 10 * MINUTE, now - HOUR, PAGE_REV)
    .first<{ last10: number; last60: number; remaining: number }>();
  if (!r) return;
  const perMin = r.last10 / 10;
  console.log(
    JSON.stringify({
      msg: "mirror backfill",
      dispatched,
      pages_last_10min: r.last10,
      pages_last_hour: r.last60,
      pages_per_min: Math.round(perMin * 10) / 10,
      remaining: r.remaining,
      eta_hours: perMin > 0 ? Math.round((r.remaining / perMin / 60) * 10) / 10 : null,
    }),
  );
}

/** One cron run of the sync. Never throws: a failed step is logged and the next run resumes. */
export async function runMirror(env: Env, opts: RunOptions = {}): Promise<void> {
  const clock = opts.clock ?? Date.now;
  const now = opts.now ?? clock();
  // Housekeeping and dispatch run whatever the sync's state: neither talks to mxb-mods.com.
  const dispatchStep = async () => {
    try {
      await wantLiveTracks(env, now);
      await evictUnused(env, now);
      await dispatch(env, now);
    } catch (err) {
      console.error(JSON.stringify({ msg: "mirror dispatch failed", error: String(err) }));
    }
  };
  if ((env.MXB_MIRROR ?? "off") !== "on") return dispatchStep();
  if (await coolingUntil(env, now)) return dispatchStep();

  const f = opts.fetch ?? globalFetch;
  const s: Sync = {
    env,
    now,
    clock: opts.now !== undefined && !opts.clock ? () => now : clock,
    fetch: f,
    wait: opts.wait ?? sleep,
    rules: await loadRobots(env, now, f),
    sent: 0,
    images: 0,
    spacing: SPACING_MS,
  };
  // Page reads go to the queue before the walk (so the consumers have work while it runs) and
  // after it (for what it found). Without a queue the cron reads a few itself.
  let dispatched = 0;
  const pages = async () => {
    if (env.MIRROR_QUEUE) dispatched += await dispatchPages(env, s.clock());
    else await readPages(s);
  };
  // Each step on its own: one that fails (a D1 error, a bad listing) must not keep the others
  // from running. A refusal from the site stops them all, by design.
  const steps: [string, () => Promise<void>][] = [
    ["pages", pages],
    ["discover", async () => {
      const tree = await loadCategories(s);
      if (tree.size > 0) await discover(s, tree);
    }],
    ["sweep", () => sweep(s)],
    ["pages", pages],
  ];
  for (const [step, run] of steps) {
    try {
      await run();
    } catch (err) {
      console.error(JSON.stringify({ msg: "mirror sync step failed", step, error: String(err) }));
      if (err instanceof Refused) break;
    }
  }
  try {
    await logBackfill(env, s.clock(), dispatched);
  } catch (err) {
    console.error(JSON.stringify({ msg: "mirror backfill log failed", error: String(err) }));
  }
  await dispatchStep();
}
