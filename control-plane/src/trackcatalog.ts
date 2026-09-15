/**
 * Which catalogue product a server's track is, for the app's server tiles.
 *
 * A server names its track by an internal id (`farm14`, `kmx_ponca_city`) and nothing else.
 * Working that out on every player's machine meant every install asking mxb-mods.com the same
 * questions, which is how players get their address blocked. So it is asked once, here: the
 * app sends the ids it lacks and gets back what is already known, and the unknown ones are
 * queued. The cron works the queue a few ids at a time and keeps a copy of each picture in R2,
 * so tiles load from us and never from the catalogue site.
 *
 * mxb-mods.com goes first, because that is where tracks come from and they are free there. The
 * shop's catalogue dump follows, when its credential is configured, and carries the price.
 */

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

/** A found track is looked up again after this, in case the post moved or the price changed. */
const HIT_TTL_MS = 7 * DAY;
/** A track nothing matched is tried again after this, in case it was published since. */
const MISS_TTL_MS = DAY;
/** A lookup the site refused (a challenge, an outage) is retried after this. */
const RETRY_MS = HOUR;
/** Rows nobody has asked about for this long are forgotten, picture and all. */
const FORGET_AFTER_MS = 90 * DAY;

/** Lookups per cron run. Each is one or two requests to mxb-mods.com, one after another. */
const RESOLVE_PER_RUN = 12;
/** Ids one request may ask about. A server list holds far fewer distinct tracks. */
export const MAX_IDS = 100;
/** Ceiling on ids waiting to be looked up, so a flood of made-up ids can't grow the table. */
const MAX_QUEUED = 5000;
const MAX_IMAGE_BYTES = 4 * 1024 * 1024;

const MODS_BASE = "https://mxb-mods.com";
/** mxb-mods.com's Tracks category. An unscoped search for `forest` comes back liveries. */
const MODS_TRACKS_CATEGORY = 22;
const SHOP_DUMP = "https://mxbikes-shop.com/frostmod-mods.json";
const UA = "mxbsecure-trackcatalog/1 (+https://mxbsecure.com)";

export interface Price {
  currency: string | null;
  base: number | null;
  sale: number | null;
  free: boolean;
}

/** One track as the app gets it. */
export interface TrackProduct {
  source: "mods" | "shop";
  exact: boolean;
  name: string;
  url: string;
  slug: string | null;
  /** Our copy of its picture. */
  image: string | null;
  price: Price | null;
}

interface Row {
  track_key: string;
  track_id: string;
  source: string;
  exact: number;
  name: string | null;
  url: string | null;
  slug: string | null;
  image_src: string | null;
  has_image: number;
  price: string | null;
  requested_at: number;
}

/** What two spellings of one track share: its letters and digits, lowercased. The app's
 *  `tracksource::key`, so both sides agree which row an id is. */
export function trackKey(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]/gu, "")
    .slice(0, 64);
}

/** Lowercase, and everything that isn't a letter or digit reduced to one space. */
export function fold(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

/**
 * Whether a catalogue title answers to a track id: `true` for the same name, `false` for a
 * resemblance worth offering, `null` for a title that merely shares a word with it.
 *
 * A resemblance has to carry every word of the id, and the id has to be at least half the
 * title: "forest" resembles "Forest MX" but not "Club Forest Racing Park Winter Edition".
 * The app's `name_answers`, plus one thing: `farm14` is the same name as "Farm 14".
 */
export function nameAnswers(id: string, title: string): boolean | null {
  const want = fold(id);
  const got = fold(title);
  if (!want || !got) return null;
  if (want === got || want.replace(/ /g, "") === got.replace(/ /g, "")) return true;
  const wanted = want.split(" ");
  const have = got.split(" ");
  if (!wanted.every((w) => have.includes(w))) return null;
  return wanted.length * 2 >= have.length ? false : null;
}

/** The best of a page of results: the first exact match, else the first resemblance. */
export function pickBest<T>(
  id: string,
  items: T[],
  title: (t: T) => string,
  eligible: (t: T) => boolean = () => true,
): [T, boolean] | null {
  let resembles: T | null = null;
  for (const item of items) {
    if (!eligible(item)) continue;
    const answer = nameAnswers(id, title(item));
    if (answer === true) return [item, true];
    if (answer === false && resembles === null) resembles = item;
  }
  return resembles === null ? null : [resembles, false];
}

/** What to type into the site's search. `farm14` also as "farm 14", since the site's search
 *  matches whole words and a post is called "Farm 14". */
export function searchWords(id: string): string[] {
  const words = id.replace(/[_\-.]+/g, " ").trim();
  const split = words.replace(/(\p{L})(\p{N})/gu, "$1 $2").replace(/(\p{N})(\p{L})/gu, "$1 $2");
  return split === words ? [words] : [words, split];
}

/** A track id as a server sends it: short, and no control characters. Spaces are fine. */
function isTrackId(value: string): boolean {
  return value.length > 0 && value.length <= 64 && !hasControl(value);
}

function hasControl(value: string): boolean {
  for (const ch of value) {
    const c = ch.codePointAt(0)!;
    if (c < 0x20 || c === 0x7f) return true;
  }
  return false;
}


/** `GET /v1/tracks?id=…&id=…`: what we know about each, keyed by the id as asked. Ids not
 *  looked up yet are left out and queued, so asking again later fills them in. */
export async function getTracks(url: URL, env: Env): Promise<Response> {
  const ids = [...new Set(url.searchParams.getAll("id").map((s) => s.trim()))]
    .filter(isTrackId)
    .slice(0, MAX_IDS);
  const byKey = new Map<string, string[]>();
  for (const id of ids) {
    const key = trackKey(id);
    if (key) byKey.set(key, [...(byKey.get(key) ?? []), id]);
  }
  const keys = [...byKey.keys()];
  if (keys.length === 0) return json(200, { tracks: {} });

  const marks = keys.map(() => "?").join(",");
  const { results } = await env.DB.prepare(
    `SELECT * FROM track_catalog WHERE track_key IN (${marks})`,
  )
    .bind(...keys)
    .all<Row>();
  const now = Date.now();
  const known = new Set(results.map((r) => r.track_key));

  // Being asked about is what keeps a row. Written at most daily per row, not per request.
  if (known.size > 0) {
    const k = [...known];
    await env.DB.prepare(
      `UPDATE track_catalog SET requested_at = ? WHERE requested_at < ? AND track_key IN (${k
        .map(() => "?")
        .join(",")})`,
    )
      .bind(now, now - DAY, ...k)
      .run();
  }

  const unknown = keys.filter((k) => !known.has(k));
  if (unknown.length > 0) {
    const queued = await env.DB.prepare(
      "SELECT COUNT(*) AS n FROM track_catalog WHERE checked_at = 0",
    ).first<{ n: number }>();
    if ((queued?.n ?? 0) < MAX_QUEUED) {
      await env.DB.batch(
        unknown.map((k) =>
          env.DB.prepare(
            "INSERT OR IGNORE INTO track_catalog (track_key, track_id, requested_at) VALUES (?, ?, ?)",
          ).bind(k, byKey.get(k)![0], now),
        ),
      );
    }
  }

  const tracks: Record<string, TrackProduct> = {};
  for (const row of results) {
    const product = productOf(row, url.origin);
    if (!product) continue;
    for (const id of byKey.get(row.track_key) ?? []) tracks[id] = product;
  }
  return new Response(JSON.stringify({ tracks }), {
    status: 200,
    headers: { "content-type": "application/json", "cache-control": "public, max-age=300" },
  });
}

function productOf(row: Row, origin: string): TrackProduct | null {
  if ((row.source !== "mods" && row.source !== "shop") || !row.name || !row.url) return null;
  let price: Price | null = null;
  try {
    price = row.price ? (JSON.parse(row.price) as Price) : null;
  } catch {
    price = null;
  }
  return {
    source: row.source,
    exact: row.exact === 1,
    name: row.name,
    url: row.url,
    slug: row.slug,
    image: row.has_image ? `${origin}/v1/tracks/art/${encodeURIComponent(row.track_key)}` : null,
    price,
  };
}

/** `GET /v1/tracks/art/<key>`: our copy of a track's picture. */
export async function trackArt(key: string, env: Env): Promise<Response> {
  if (!/^[\p{L}\p{N}]{1,64}$/u.test(key)) return json(400, { error: "not a track" });
  const obj = await env.PAINTS.get(artKey(key));
  if (!obj) return json(404, { error: "no picture" });
  return new Response(obj.body, {
    headers: {
      "content-type": obj.httpMetadata?.contentType ?? "image/jpeg",
      "cache-control": "public, max-age=86400",
      etag: obj.httpEtag,
    },
  });
}

function artKey(key: string): string {
  return `trackart/${key}`;
}

// ───────────────────────────────── the cron ─────────────────────────────────

interface Answer {
  source: "mods" | "shop";
  exact: boolean;
  name: string;
  url: string;
  slug: string | null;
  image: string | null;
  price: Price | null;
}

/** Look up the tracks that are due, and forget the ones nobody asks about any more. */
export async function resolveTrackCatalog(env: Env): Promise<void> {
  try {
    const now = Date.now();
    await forget(env, now);
    const { results } = await env.DB.prepare(
      "SELECT * FROM track_catalog WHERE due_at <= ? ORDER BY due_at, requested_at DESC LIMIT ?",
    )
      .bind(now, RESOLVE_PER_RUN)
      .all<Row>();
    if (results.length === 0) return;

    // Fetched once per run, and only if some track gets as far as needing it.
    let shop: Promise<ShopItem[] | null> | null = null;
    const shopItems = () => (shop ??= shopCatalog(env));
    for (const row of results) await resolveOne(env, row, shopItems, now);
  } catch (err) {
    console.error(JSON.stringify({ msg: "track catalog run failed", error: String(err) }));
  }
}

async function resolveOne(
  env: Env,
  row: Row,
  shopItems: () => Promise<ShopItem[] | null>,
  now: number,
): Promise<void> {
  let answer: Answer | null = null;
  let refused = false;

  for (const words of searchWords(row.track_id)) {
    const hits = await searchMods(words);
    if (hits === null) {
      refused = true;
      break;
    }
    const best = pickBest(row.track_id, hits, (h) => h.title);
    if (best) {
      const [hit, exact] = best;
      answer = {
        source: "mods",
        exact,
        name: hit.title,
        url: hit.url,
        slug: hit.slug,
        image: hit.image,
        price: null,
      };
      break;
    }
  }

  if (!answer && !refused) {
    const items = await shopItems();
    const best = items ? pickBest(row.track_id, items, (i) => i.title, (i) => sellsTracks(i.categories)) : null;
    if (best) {
      const [item, exact] = best;
      answer = {
        source: "shop",
        exact,
        name: item.title,
        url: item.url,
        slug: null,
        image: item.image,
        price: item.price,
      };
    }
  }

  if (refused) {
    // Whatever we knew stands; only when to try again moves.
    await env.DB.prepare("UPDATE track_catalog SET due_at = ? WHERE track_key = ?")
      .bind(now + RETRY_MS, row.track_key)
      .run();
    return;
  }

  if (!answer) {
    // A track that used to match keeps its answer: a search that misses once is weaker
    // evidence than the match we already had.
    await env.DB.prepare("UPDATE track_catalog SET checked_at = ?, due_at = ? WHERE track_key = ?")
      .bind(now, now + (row.source ? HIT_TTL_MS : MISS_TTL_MS), row.track_key)
      .run();
    return;
  }

  const hasImage = await copyImage(env, row.track_key, answer.image, row);
  await env.DB.prepare(
    "UPDATE track_catalog SET source = ?, exact = ?, name = ?, url = ?, slug = ?, image_src = ?," +
      " has_image = ?, price = ?, checked_at = ?, due_at = ? WHERE track_key = ?",
  )
    .bind(
      answer.source,
      answer.exact ? 1 : 0,
      answer.name,
      answer.url,
      answer.slug,
      answer.image,
      hasImage ? 1 : 0,
      answer.price ? JSON.stringify(answer.price) : null,
      now,
      now + HIT_TTL_MS,
      row.track_key,
    )
    .run();
}

/** Copy the picture into R2. Skipped when it's the one we already hold. */
async function copyImage(env: Env, key: string, src: string | null, row: Row): Promise<boolean> {
  if (!src) {
    if (row.has_image) await env.PAINTS.delete(artKey(key));
    return false;
  }
  if (row.has_image && row.image_src === src) return true;
  try {
    const res = await fetch(src, { headers: { "user-agent": UA } });
    const type = res.headers.get("content-type") ?? "";
    if (!res.ok || !type.startsWith("image/")) return !!row.has_image;
    const bytes = await res.arrayBuffer();
    if (bytes.byteLength === 0 || bytes.byteLength > MAX_IMAGE_BYTES) return !!row.has_image;
    await env.PAINTS.put(artKey(key), bytes, { httpMetadata: { contentType: type } });
    return true;
  } catch {
    return !!row.has_image;
  }
}

async function forget(env: Env, now: number): Promise<void> {
  const { results } = await env.DB.prepare(
    "SELECT track_key FROM track_catalog WHERE requested_at < ? AND has_image = 1 LIMIT 100",
  )
    .bind(now - FORGET_AFTER_MS)
    .all<{ track_key: string }>();
  if (results.length > 0) await env.PAINTS.delete(results.map((r) => artKey(r.track_key)));
  await env.DB.prepare("DELETE FROM track_catalog WHERE requested_at < ?")
    .bind(now - FORGET_AFTER_MS)
    .run();
}

// ───────────────────────────────── mxb-mods.com ─────────────────────────────────

interface ModsHit {
  title: string;
  url: string;
  slug: string | null;
  image: string | null;
}

/** One search of the Tracks category. `null` when the site wouldn't answer. */
async function searchMods(words: string): Promise<ModsHit[] | null> {
  const u = new URL("/wp-json/wp/v2/posts", MODS_BASE);
  u.searchParams.set("categories", String(MODS_TRACKS_CATEGORY));
  u.searchParams.set("search", words);
  u.searchParams.set("per_page", "20");
  u.searchParams.set("_embed", "wp:featuredmedia");
  try {
    const res = await fetch(u, { headers: { "user-agent": UA, accept: "application/json" } });
    if (!res.ok) return null;
    const posts = (await res.json()) as unknown;
    if (!Array.isArray(posts)) return null;
    return posts.flatMap((p: Record<string, any>) => {
      const title = decodeEntities(String(p?.title?.rendered ?? "")).trim();
      const url = typeof p?.link === "string" ? p.link : "";
      if (!title || !url.startsWith("https://")) return [];
      return [
        {
          title,
          url,
          slug: typeof p?.slug === "string" ? p.slug : null,
          image: featuredImage(p),
        },
      ];
    });
  } catch {
    return null;
  }
}

/** The post's picture, at the size a tile needs rather than the full upload. */
function featuredImage(post: Record<string, any>): string | null {
  const media = post?._embedded?.["wp:featuredmedia"]?.[0];
  const sizes = media?.media_details?.sizes;
  for (const size of ["medium_large", "large", "full"]) {
    const src = sizes?.[size]?.source_url;
    if (typeof src === "string" && src.startsWith("https://")) return src;
  }
  const src = media?.source_url;
  return typeof src === "string" && src.startsWith("https://") ? src : null;
}

const ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

export function decodeEntities(s: string): string {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, body: string) => {
    if (body[0] === "#") {
      const code = body[1] === "x" || body[1] === "X" ? parseInt(body.slice(2), 16) : parseInt(body.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }
    return ENTITIES[body.toLowerCase()] ?? whole;
  });
}

// ───────────────────────────────── the shop ─────────────────────────────────

interface ShopItem {
  title: string;
  url: string;
  image: string | null;
  categories: string[];
  price: Price;
}

/** Whether a product's categories say it's a track. An empty list is let through. */
function sellsTracks(categories: string[]): boolean {
  return categories.length === 0 || categories.some((c) => c.toLowerCase().includes("track"));
}

/** The shop's catalogue dump, when its credential is configured. `null` otherwise. */
async function shopCatalog(env: Env): Promise<ShopItem[] | null> {
  const req = shopRequest(env);
  if (!req) return null;
  try {
    const res = await fetch(req);
    if (!res.ok) return null;
    return parseShopDump(await res.json());
  } catch {
    return null;
  }
}

/**
 * The dump request, authenticated the way `SHOP_CATALOG_AUTH` says: `header:<name>`,
 * `basic:<user>`, `bearer` or `query:<name>`, with `SHOP_CATALOG_KEY` as the key.
 */
function shopRequest(env: Env): Request | null {
  const scheme = env.SHOP_CATALOG_AUTH?.trim();
  const key = env.SHOP_CATALOG_KEY?.trim();
  if (!scheme || !key) return null;
  const url = new URL(SHOP_DUMP);
  const headers = new Headers({ "user-agent": UA, accept: "application/json" });
  const [kind, name = ""] = scheme.split(":", 2);
  if (kind === "header" && name) headers.set(name, key);
  else if (kind === "basic" && name) headers.set("authorization", `Basic ${btoa(`${name}:${key}`)}`);
  else if (kind === "bearer") headers.set("authorization", `Bearer ${key}`);
  else if (kind === "query" && name) url.searchParams.set(name, key);
  else return null;
  return new Request(url, { headers });
}

function num(v: unknown): number | null {
  const n = typeof v === "string" ? Number(v) : v;
  return typeof n === "number" && Number.isFinite(n) ? n : null;
}

/** The dump's items, as permissively as the app reads them (`shop_catalog::map_mod`). */
export function parseShopDump(dump: unknown): ShopItem[] {
  const d = (dump ?? {}) as Record<string, any>;
  const currency = typeof d.currency === "string" ? d.currency : null;

  const names = new Map<number, string>();
  const walk = (cats: unknown) => {
    if (!Array.isArray(cats)) return;
    for (const c of cats) {
      const id = num(c?.id);
      const name = c?.name ?? c?.title ?? c?.label;
      if (id !== null && typeof name === "string") names.set(id, name);
      walk(c?.children);
    }
  };
  walk(d.categories);

  const mods: unknown[] = Array.isArray(d.mods) ? d.mods : [];
  return mods.flatMap((raw) => {
    const m = (raw ?? {}) as Record<string, any>;
    const title = String(m.title ?? m.name ?? m.post_title ?? "").trim();
    const url = m.url ?? m.permalink ?? m.link ?? m.product_url ?? m.href;
    if (!title || typeof url !== "string" || !url.startsWith("https://")) return [];
    const image = m.image ?? m.thumbnail ?? m.thumb ?? m.featured_image ?? m.img;
    const rawCats = m.categories ?? m.category ?? m.category_ids;
    const categories = (Array.isArray(rawCats) ? rawCats : rawCats == null ? [] : [rawCats]).flatMap(
      (c: unknown) => {
        if (typeof c === "string" && Number.isNaN(Number(c))) return [c];
        const id = num(typeof c === "object" && c ? (c as Record<string, unknown>).id : c);
        const named = typeof c === "object" && c ? (c as Record<string, unknown>).name : null;
        if (typeof named === "string") return [named];
        return id !== null && names.has(id) ? [names.get(id)!] : [];
      },
    );
    return [
      {
        title: decodeEntities(title),
        url,
        image: typeof image === "string" && image.startsWith("https://") ? image : null,
        categories,
        price: {
          currency,
          base: num(m.price),
          sale: num(m.sale_price),
          free: m.free === true,
        },
      },
    ];
  });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
