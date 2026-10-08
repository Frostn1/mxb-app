/**
 * The mod mirror's sync: mxb-mods.com's catalogue, walked politely and copied into R2.
 *
 * Three steps, each with its own budget per cron run so one run can never blow the Worker's
 * CPU or subrequest limits, and each resumable from D1 alone:
 *
 *  1. **Discover.** Walk the REST listing of everything modified since the last finished walk
 *     (`Listing`). A new or changed post is upserted and its page marked due. A second, cheap
 *     cursor sweeps every post id so `last_seen` stays true and deleted posts drop out of search.
 *  2. **Read pages.** The download links, version and byline are only on the rendered page
 *     (the REST body has none of them), so each due page is fetched once, parsed the way the
 *     app parses it (`apps/manager/src-tauri/src/mods/mxb.rs`), and its options upserted. An
 *     unchanged link keeps its mirrored file; a changed one is fetched again.
 *  3. **Dispatch.** Files that are due are leased and handed to the `mxb-mirror` queue, one
 *     message per file. The consumer (`mirrorfetch.ts`) streams each into R2 by SHA-256.
 *
 * Everything sent to mxb-mods.com carries a named user agent, is spaced out, and is checked
 * against the site's robots.txt first.
 */

import { decodeEntities } from "./trackcatalog";
import { evictUnused, wantLiveTracks } from "./mirrorpolicy";
import { newPublicId } from "./modids";

export const UA = "mxbsecure-mirror/1 (+https://mxbsecure.com/mods)";
/** The product token robots.txt groups are matched against. */
const ROBOTS_TOKEN = "mxbsecure-mirror";
const MODS_BASE = "https://mxb-mods.com";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Listing pages walked per run, 50 posts each. */
const LIST_PAGES_PER_RUN = 4;
const LIST_PER_PAGE = 50;
/** Id-sweep pages per run, 100 ids each. */
const SWEEP_PAGES_PER_RUN = 1;
/** Post pages read per run, every 10 minutes: ~5,700 a day, so the ~13k-post backfill takes
 *  about two and a half days, on purpose. With their thumbnails that is ~80 requests 3 s
 *  apart, four minutes of each ten. */
const PAGES_PER_RUN = 40;
/** Files handed to the queue per run. The queue consumer's own concurrency is the other cap. */
const DISPATCH_PER_RUN = 20;
/** Pause between two requests to mxb-mods.com. A run makes up to ~90, so under five minutes. */
const SPACING_MS = 3000;
/** How long the sync leaves the site alone after it refuses a request. */
const COOLDOWN_MS = 2 * HOUR;
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
  /** Requests made to mxb-mods.com this run. */
  sent: number;
}

/** The site turned us away (403/429/503): the run stops and the next ones wait. */
class Refused extends Error {}

/**
 * One request to mxb-mods.com: robots-checked, spaced, named. `null` when robots says no.
 * A refusal puts the whole sync on a cooldown, because a site that has started saying no to
 * one request will say no to the next, and asking again is how an address gets blocked.
 */
async function modsGet(s: Sync, url: URL | string, accept = "application/json"): Promise<Response | null> {
  const u = typeof url === "string" ? new URL(url, MODS_BASE) : url;
  if (!robotsAllows(s.rules, u.pathname + u.search)) return null;
  if (s.sent > 0) await s.wait(SPACING_MS);
  s.sent++;
  // Called as a plain function, never as `s.fetch(…)`: the runtime's fetch throws "Illegal
  // invocation" when its `this` is anything but the global scope.
  const get = s.fetch;
  const res = await get(u.toString(), { headers: { "user-agent": UA, accept } });
  if (res.status === 403 || res.status === 429 || res.status === 503) {
    const after = Number(res.headers.get("retry-after"));
    const wait = Math.max(COOLDOWN_MS, Number.isFinite(after) ? after * 1000 : 0);
    await setState(s.env, "cooldown", { until: s.now + wait, status: res.status });
    throw new Refused(`mxb-mods answered ${res.status}`);
  }
  return res;
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

async function discover(s: Sync, tree: Map<number, Category>): Promise<void> {
  const st = (await getState<Listing>(s.env, "listing")) ?? { hwm: "", walk: null };
  for (let n = 0; n < LIST_PAGES_PER_RUN; n++) {
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

interface DueAsset {
  id: number;
  source_url: string;
  thumb_src: string | null;
  thumb_key: string | null;
  page_attempts: number;
}

async function readPages(s: Sync): Promise<void> {
  const { results } = await s.env.DB.prepare(
    `SELECT id, source_url, thumb_src, thumb_key, page_attempts FROM mod_assets
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
  const html = await res.text();
  const downloads = parseDownloads(html);
  if (downloads.length === 0 && isChallenge(html)) return pageRetry(s, asset, "challenge page");

  // The links first: they are what the page is for, and nothing after may lose them.
  await writeMirrorVersion(s.env, asset.id, parseVersion(html), downloads, s.now);
  const src = asset.thumb_src ?? parseImage(html);
  const thumb = asset.thumb_key ? null : await copyThumb(s, src);
  await s.env.DB.prepare(
    `UPDATE mod_assets SET author = COALESCE(?, author), thumb_src = COALESCE(thumb_src, ?),
       thumb_sha = COALESCE(?, thumb_sha), thumb_key = COALESCE(?, thumb_key),
       page_status = 'ok', page_attempts = 0, page_error = NULL WHERE id = ?`,
  )
    .bind(parseAuthor(html), src, thumb?.sha ?? null, thumb?.key ?? null, asset.id)
    .run();
}

async function pageRetry(s: Sync, asset: DueAsset, error: string): Promise<void> {
  const attempts = asset.page_attempts + 1;
  const status = attempts >= PAGE_MAX_ATTEMPTS ? "gone" : "retry";
  await s.env.DB.prepare(
    "UPDATE mod_assets SET page_status = ?, page_attempts = ?, page_due_at = ?, page_error = ? WHERE id = ?",
  )
    .bind(status, attempts, s.now + backoff(attempts), error.slice(0, 300), asset.id)
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
  const known = await s.env.DB.prepare(
    "SELECT thumb_sha AS sha, thumb_key AS key FROM mod_assets WHERE thumb_src = ? AND thumb_key IS NOT NULL LIMIT 1",
  )
    .bind(src)
    .first<Thumb>();
  if (known) return known;
  try {
    const u = new URL(src);
    if (u.hostname !== "mxb-mods.com") return null;
    const res = await modsGet(s, u, "image/*");
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

/** A queue message: one mirrored file to fetch, or one upload to verify. */
export type MirrorJob =
  | { kind: "file"; version: number; idx: number; part: number }
  | { kind: "upload"; id: string };

/** Lease the current versions' files that are due and hand them to the queue. */
export async function dispatch(env: Env, now: number, limit = DISPATCH_PER_RUN): Promise<number> {
  if (!env.MIRROR_QUEUE) return 0;
  const { results } = await env.DB.prepare(
    `SELECT f.version_id, f.idx, f.part FROM mod_files f
     JOIN mod_assets a ON a.current_version = f.version_id
     WHERE a.source = 'mirror' AND a.page_status = 'ok' AND a.state = 'active' AND f.url IS NOT NULL AND (
       (f.status IN ('pending', 'retry') AND f.due_at <= ?1) OR (f.status = 'queued' AND f.leased_until < ?1))
     ORDER BY f.due_at, a.id DESC, f.idx, f.part LIMIT ?2`,
  )
    .bind(now, limit)
    .all<{ version_id: number; idx: number; part: number }>();
  if (results.length === 0) return 0;
  await env.DB.batch(
    results.map((r) =>
      env.DB.prepare(
        "UPDATE mod_files SET status = 'queued', leased_until = ? WHERE version_id = ? AND idx = ? AND part = ?",
      ).bind(now + LEASE_MS, r.version_id, r.idx, r.part),
    ),
  );
  await env.MIRROR_QUEUE.sendBatch(
    results.map((r) => ({ body: { kind: "file", version: r.version_id, idx: r.idx, part: r.part } satisfies MirrorJob })),
  );
  return results.length;
}

// ───────────────────────────── the run ─────────────────────────────

export interface RunOptions {
  now?: number;
  fetch?: typeof fetch;
  wait?: (ms: number) => Promise<void>;
}

/** One cron run of the sync. Never throws: a failed step is logged and the next run resumes. */
export async function runMirror(env: Env, opts: RunOptions = {}): Promise<void> {
  const now = opts.now ?? Date.now();
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
  const cooldown = await getState<{ until: number }>(env, "cooldown");
  if (cooldown && cooldown.until > now) return dispatchStep();

  const f = opts.fetch ?? globalFetch;
  const s: Sync = {
    env,
    now,
    fetch: f,
    wait: opts.wait ?? ((ms) => new Promise((r) => setTimeout(r, ms))),
    rules: await loadRobots(env, now, f),
    sent: 0,
  };
  // Each step on its own: one that fails (a D1 error, a bad listing) must not keep the others
  // from running. A refusal from the site stops them all, by design.
  const steps: [string, () => Promise<void>][] = [
    ["discover", async () => {
      const tree = await loadCategories(s);
      if (tree.size > 0) await discover(s, tree);
    }],
    ["sweep", () => sweep(s)],
    ["pages", () => readPages(s)],
  ];
  for (const [step, run] of steps) {
    try {
      await run();
    } catch (err) {
      console.error(JSON.stringify({ msg: "mirror sync step failed", step, error: String(err) }));
      if (err instanceof Refused) break;
    }
  }
  await dispatchStep();
}
