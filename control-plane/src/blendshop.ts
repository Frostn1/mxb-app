/**
 * The Blends shop on mxbsecure.com/blends: one-off purchases of a part's source pack (.blend,
 * FBX, PSD, textures), sold by Creste LLC through Stripe Checkout, and downloaded from the
 * private bucket by the Steam account that bought it.
 *
 * Off unless `MXB_BLENDS_SHOP` is `test` or `live` AND the Stripe key is of that kind
 * (`sk_test_`/`rk_test_` for test, `sk_live_`/`rk_live_` for live) AND the webhook secret and the
 * site's session key are set. A live key with the switch on `test` keeps the shop shut, so the
 * hosting key can't take real money for a pack by accident.
 *
 * - Prices are the site's (`src/lib/blend-list.ts` in mxbsecure-site, published at
 *   `/shop/blends.json`). The browser sends a slug only; the price is read here.
 * - Checkout is `mode: payment` with Stripe Tax on, an invoice (so the receipt is in the billing
 *   portal) and the order id in its metadata. Nothing is granted until the webhook says paid.
 * - The webhook (`stripeWebhook` in `billing.ts`, same endpoint and secret as hosting) records
 *   the order and the entitlement once per event id; a refund or a dispute takes it away.
 * - A download is a short-lived signed link for the signed-in buyer, counted against a limit.
 *   The zip is streamed from `ASSET_LOCKED` (mxb-private) under `blends/<slug>.zip` and marked
 *   with the order on the way out (`zipmark.ts`). Nothing in the public bucket.
 */

import { cors, refuseCrossSiteWrite } from "./assets";
import { tokenMatches } from "./auth";
import { BANNED, isBanned } from "./bans";
import { customerFor, stripe } from "./billing";
import { alert, type Deps } from "./hosting";
import { markZip } from "./zipmark";
import { clearedCookie, LEGACY_SESSION_COOKIE, SESSION_COOKIE, webSession } from "./websession";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** How long a download link works. */
export const LINK_TTL_MS = 10 * MINUTE;
/** Download links one account may ask for in an hour, across every pack. */
export const LINKS_PER_HOUR = 10;
/** Download links for one pack in a day. */
export const LINKS_PER_PACK_PER_DAY = 5;
/** How long the Checkout page stays open (Stripe allows 30 minutes to 24 hours). */
const CHECKOUT_TTL_S = 60 * 60;
/** How long the price list is kept before it is read from the site again. */
const CATALOG_TTL_MS = 5 * MINUTE;
/** Stripe's tax code for electronically supplied goods; `MXB_BLENDS_TAX_CODE` overrides it. */
const DEFAULT_TAX_CODE = "txcd_10000000";
const SITE = "https://mxbsecure.com";
export const ZIP_PREFIX = "blends/";

const SLUG_RE = /^[a-z0-9][a-z0-9-]{0,63}$/;
const TERMS_RE = /^\d{4}-\d{2}-\d{2}$/;
const FILE_RE = /^\/v1\/web\/blends\/file\/([A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)$/;

export type OrderStatus = "pending" | "paid" | "expired" | "refunded" | "disputed";

export interface ShopConfig {
  mode: "test" | "live";
  secretKey: string;
  webhookSecret: string;
  sessionKey: string;
  catalogUrl: string;
  taxCode: string;
  site: string;
}

/** The shop's config, or null when it is shut (see the top of this file). */
export function shopConfig(env: Env): ShopConfig | null {
  const mode = env.MXB_BLENDS_SHOP?.trim();
  if (mode !== "test" && mode !== "live") return null;
  const secretKey = env.STRIPE_SECRET_KEY?.trim();
  const webhookSecret = env.STRIPE_WEBHOOK_SECRET?.trim();
  const sessionKey = env.MXB_WEB_SESSION_KEY;
  if (!secretKey || !webhookSecret || !sessionKey) return null;
  if (!secretKey.startsWith(`sk_${mode}_`) && !secretKey.startsWith(`rk_${mode}_`)) return null;
  const site = (env.MXB_SITE_ORIGIN || SITE).replace(/\/+$/, "");
  return {
    mode,
    secretKey,
    webhookSecret,
    sessionKey,
    catalogUrl: env.MXB_BLENDS_CATALOG_URL?.trim() || `${site}/shop/blends.json`,
    taxCode: env.MXB_BLENDS_TAX_CODE?.trim() || DEFAULT_TAX_CODE,
    site,
  };
}

// ---- The price list ------------------------------------------------------------------------

export interface CatalogItem {
  slug: string;
  name: string;
  priceCents: number;
}

/** The site's `/shop/blends.json`, checked. Items without a sane price are not for sale. */
export function parseCatalog(raw: unknown): Map<string, CatalogItem> | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as { format?: unknown; items?: unknown };
  if (r.format !== "mxb-blends/1" || !Array.isArray(r.items)) return null;
  const out = new Map<string, CatalogItem>();
  for (const it of r.items as unknown[]) {
    const x = (it ?? {}) as Record<string, unknown>;
    const slug = typeof x.slug === "string" && SLUG_RE.test(x.slug) ? x.slug : null;
    const name = typeof x.name === "string" ? x.name.replace(/[\u0000-\u001f\u007f]+/g, " ").trim().slice(0, 120) : "";
    const price = x.priceCents;
    if (!slug || !name || typeof price !== "number" || !Number.isInteger(price) || price < 100 || price > 100_000) continue;
    out.set(slug, { slug, name, priceCents: price });
  }
  return out;
}

let catalogCache: { url: string; at: number; items: Map<string, CatalogItem> } | null = null;

/** For tests. */
export function forgetCatalog(): void {
  catalogCache = null;
}

export async function loadCatalog(cfg: ShopConfig, deps: Deps): Promise<Map<string, CatalogItem> | null> {
  const now = deps.now();
  if (catalogCache && catalogCache.url === cfg.catalogUrl && now - catalogCache.at < CATALOG_TTL_MS) return catalogCache.items;
  try {
    const res = await deps.fetch(cfg.catalogUrl, { headers: { Accept: "application/json" }, signal: AbortSignal.timeout(5000) });
    if (!res.ok) throw new Error(`catalog ${res.status}`);
    const items = parseCatalog(await res.json());
    if (!items) throw new Error("catalog unreadable");
    catalogCache = { url: cfg.catalogUrl, at: now, items };
    return items;
  } catch (err) {
    console.error(JSON.stringify({ msg: "blends catalog", error: String(err).slice(0, 200) }));
    // A stale list beats a shut shop for a few minutes of site trouble.
    return catalogCache?.url === cfg.catalogUrl ? catalogCache.items : null;
  }
}

// ---- Rows ----------------------------------------------------------------------------------

export interface OrderRow {
  id: string;
  ref: string;
  steam_id: string;
  slug: string;
  name: string;
  status: OrderStatus;
  price_cents: number;
  currency: string;
  subtotal_cents: number | null;
  tax_cents: number | null;
  total_cents: number | null;
  customer_id: string | null;
  checkout_session_id: string | null;
  payment_intent_id: string | null;
  terms_version: string;
  agreed_at: number;
  first_download_at: number | null;
  created_at: number;
  paid_at: number | null;
  closed_at: number | null;
  updated_at: number;
}

const REF_ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/** `MXBB-` and eight characters nobody misreads: what the buyer quotes and the zip carries. */
export function newRef(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  return `MXBB-${Array.from(bytes, (b) => REF_ALPHABET[b % REF_ALPHABET.length]).join("")}`;
}

async function orderBy(env: Env, column: "id" | "checkout_session_id" | "payment_intent_id", value: string): Promise<OrderRow | null> {
  return env.DB.prepare(`SELECT * FROM blend_orders WHERE ${column} = ?`).bind(value).first<OrderRow>();
}

async function owned(env: Env, steamId: string, slug: string): Promise<{ order_id: string } | null> {
  return env.DB.prepare("SELECT order_id FROM blend_entitlements WHERE steam_id = ? AND slug = ? AND revoked_at IS NULL")
    .bind(steamId, slug)
    .first<{ order_id: string }>();
}

// ---- Download links ------------------------------------------------------------------------

interface LinkClaims {
  d: number;
  o: string;
  s: string;
  slug: string;
  exp: number;
}

function b64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function unb64url(text: string): string {
  const s = text.replace(/-/g, "+").replace(/_/g, "/");
  return new TextDecoder().decode(Uint8Array.from(atob(s + "=".repeat((4 - (s.length % 4)) % 4)), (c) => c.charCodeAt(0)));
}

async function linkMac(key: string, body: string): Promise<string> {
  // Its own key, derived from the session key, so a link can never pass as a session or the other way round.
  const k = await crypto.subtle.importKey("raw", new TextEncoder().encode(`blend-download:${key}`), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return b64url(new Uint8Array(await crypto.subtle.sign("HMAC", k, new TextEncoder().encode(body))));
}

export async function sealLink(claims: LinkClaims, key: string): Promise<string> {
  const body = b64url(new TextEncoder().encode(JSON.stringify(claims)));
  return `${body}.${await linkMac(key, body)}`;
}

export async function openLink(token: string, key: string, now: number): Promise<LinkClaims | null> {
  const [body, sig, extra] = token.split(".");
  if (!body || !sig || extra !== undefined) return null;
  if (!tokenMatches(await linkMac(key, body), sig)) return null;
  try {
    const c = JSON.parse(unb64url(body)) as LinkClaims;
    if (typeof c.exp !== "number" || c.exp <= now || typeof c.o !== "string" || typeof c.s !== "string" || typeof c.slug !== "string") return null;
    return c;
  } catch {
    return null;
  }
}

// ---- Routes --------------------------------------------------------------------------------

export function isBlendShopPath(path: string): boolean {
  return path === "/v1/web/blends" || path.startsWith("/v1/web/blends/");
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
}

async function readBody(request: Request): Promise<Record<string, unknown>> {
  const text = await request.text();
  if (text.length > 4096) return {};
  try {
    const b = JSON.parse(text) as unknown;
    return b && typeof b === "object" && !Array.isArray(b) ? (b as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

export async function blendShopRoutes(request: Request, url: URL, env: Env, origin: string | null, deps: Deps): Promise<Response> {
  const path = url.pathname;
  const method = request.method;

  if (method === "OPTIONS") {
    if (request.headers.get("Origin") && !origin) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, OPTIONS", "Content-Type");
  }

  const cfg = shopConfig(env);
  const session = await webSession(request, env, deps.now());

  // What the page draws from: whether the shop is open and what this account owns.
  if (method === "GET" && path === "/v1/web/blends") {
    if (!cfg) return cors(json(200, { enabled: false, mode: null, owned: [] }), origin);
    const rows = session
      ? await env.DB.prepare("SELECT slug FROM blend_entitlements WHERE steam_id = ? AND revoked_at IS NULL").bind(session.steamId).all<{ slug: string }>()
      : { results: [] };
    return cors(json(200, { enabled: true, mode: cfg.mode, owned: rows.results.map((r) => r.slug) }), origin);
  }

  // The file itself: a top-level navigation, so no CORS, and the cookie must be the buyer's.
  const file = FILE_RE.exec(path);
  if (method === "GET" && file) return serveFile(env, deps, cfg, session?.steamId ?? null, file[1]);

  if (method === "POST") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
  }
  if (!session) return cors(json(401, { error: "Sign in with Steam first." }), origin);

  if (method === "GET" && path === "/v1/web/blends/orders") return cors(json(200, await myOrders(env, session.steamId)), origin);

  if (method === "POST" && path === "/v1/web/blends/account/delete") {
    const body = await readBody(request);
    if (body.confirm !== "DELETE") return cors(json(400, { error: "Confirm with DELETE." }), origin);
    const res = json(200, await eraseShopAccount(env, deps, session.steamId));
    res.headers.append("Set-Cookie", clearedCookie(SESSION_COOKIE));
    res.headers.append("Set-Cookie", clearedCookie(LEGACY_SESSION_COOKIE));
    return cors(res, origin);
  }

  if (!cfg) return cors(json(503, { error: "The shop is closed right now." }), origin);

  if (method === "POST" && path === "/v1/web/blends/checkout") {
    const r = await startCheckout(env, deps, cfg, session.steamId, await readBody(request));
    return cors(json(r.status, r.body), origin);
  }
  if (method === "POST" && path === "/v1/web/blends/download") {
    const body = await readBody(request);
    const r = await downloadLink(env, deps, cfg, session.steamId, body.slug, url.origin);
    return cors(json(r.status, r.body), origin);
  }
  if (method === "POST" && path === "/v1/web/blends/billing") {
    const r = await billingPortal(env, deps, cfg, session.steamId);
    return cors(json(r.status, r.body), origin);
  }
  return cors(json(404, { error: "no such endpoint" }), origin);
}

type Result = { status: number; body: unknown };

/** `POST /v1/web/blends/checkout` `{ slug, agree: true, terms: "YYYY-MM-DD" }`: a Checkout URL. */
export async function startCheckout(env: Env, deps: Deps, cfg: ShopConfig, steamId: string, body: Record<string, unknown>): Promise<Result> {
  const slug = typeof body.slug === "string" && SLUG_RE.test(body.slug) ? body.slug : null;
  if (!slug) return { status: 400, body: { error: "No such Blend." } };
  if (body.agree !== true || typeof body.terms !== "string" || !TERMS_RE.test(body.terms)) {
    return { status: 400, body: { error: "Agree to the Terms and the License first." } };
  }
  if (await isBanned(env, { steamId })) return { status: 403, body: { error: BANNED } };
  const catalog = await loadCatalog(cfg, deps);
  if (!catalog) return { status: 503, body: { error: "The shop is closed right now." } };
  const item = catalog.get(slug);
  if (!item) return { status: 404, body: { error: "This Blend isn't for sale." } };
  if (await owned(env, steamId, slug)) return { status: 409, body: { error: "You own this already." } };

  const customerId = await customerFor(env, deps, cfg, steamId);
  if (!customerId) return { status: 502, body: { error: "Payment couldn't be started. Try again." } };

  const id = crypto.randomUUID();
  const ref = newRef();
  const now = deps.now();
  const meta = { kind: "blend", order_id: id, order_ref: ref, slug, steam_id: steamId };
  const session = await stripe<{ id: string; url: string }>(
    cfg,
    deps,
    "POST",
    "/checkout/sessions",
    {
      mode: "payment",
      customer: customerId,
      client_reference_id: id,
      line_items: {
        0: {
          quantity: 1,
          price_data: {
            currency: "usd",
            unit_amount: item.priceCents,
            tax_behavior: "exclusive",
            product_data: { name: `${item.name} (Blend)`, tax_code: cfg.taxCode, metadata: { slug } },
          },
        },
      },
      automatic_tax: { enabled: true },
      // Stripe Tax needs the address on the customer; Checkout asks for it and saves it there.
      customer_update: { address: "auto", name: "auto" },
      billing_address_collection: "required",
      invoice_creation: { enabled: true, invoice_data: { description: `Order ${ref}`, metadata: meta } },
      payment_intent_data: { description: `${item.name} (Blend), order ${ref}`, metadata: meta },
      metadata: meta,
      expires_at: Math.floor(now / 1000) + CHECKOUT_TTL_S,
      success_url: `${cfg.site}/account?checkout=done&order=${ref}`,
      cancel_url: `${cfg.site}/blends/${slug}?checkout=cancelled`,
    },
    `mxb-blend-checkout-${id}`,
  );
  if (!session.ok) {
    console.error(JSON.stringify({ msg: "blends checkout", status: session.status, error: session.error }));
    await alert(env, deps, "blends", `Stripe refused a Blends checkout (${session.status}): ${session.error}`);
    return { status: 502, body: { error: "Payment couldn't be started. Try again." } };
  }
  await env.DB.prepare(
    `INSERT INTO blend_orders (id, ref, steam_id, slug, name, status, price_cents, currency, customer_id, checkout_session_id,
                               terms_version, agreed_at, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, 'pending', ?, 'usd', ?, ?, ?, ?, ?, ?)`,
  )
    .bind(id, ref, steamId, slug, item.name, item.priceCents, customerId, session.body.id, body.terms, now, now, now)
    .run();
  console.log(JSON.stringify({ msg: "blends checkout", order: ref, slug }));
  return { status: 200, body: { url: session.body.url, ref } };
}

/** `GET /v1/web/blends/orders`: this account's paid, refunded and disputed orders, newest first. */
export async function myOrders(env: Env, steamId: string): Promise<{ orders: unknown[] }> {
  const rows = await env.DB.prepare(
    `SELECT o.*, e.revoked_at AS revoked_at, e.order_id AS entitled_order
       FROM blend_orders o LEFT JOIN blend_entitlements e ON e.steam_id = o.steam_id AND e.slug = o.slug
      WHERE o.steam_id = ? AND o.status IN ('paid', 'refunded', 'disputed')
      ORDER BY COALESCE(o.paid_at, o.created_at) DESC LIMIT 200`,
  )
    .bind(steamId)
    .all<OrderRow & { revoked_at: number | null; entitled_order: string | null }>();
  return {
    orders: rows.results.map((o) => ({
      ref: o.ref,
      slug: o.slug,
      name: o.name,
      status: o.status,
      currency: o.currency,
      priceCents: o.price_cents,
      taxCents: o.tax_cents,
      totalCents: o.total_cents,
      paidAt: o.paid_at,
      downloadedAt: o.first_download_at,
      downloadable: o.status === "paid" && o.entitled_order === o.id && o.revoked_at === null,
    })),
  };
}

/** `POST /v1/web/blends/download` `{ slug }`: a link that works for `LINK_TTL_MS`, for this account only. */
export async function downloadLink(env: Env, deps: Deps, cfg: ShopConfig, steamId: string, rawSlug: unknown, apiOrigin: string): Promise<Result> {
  const slug = typeof rawSlug === "string" && SLUG_RE.test(rawSlug) ? rawSlug : null;
  if (!slug) return { status: 400, body: { error: "No such Blend." } };
  const own = await owned(env, steamId, slug);
  if (!own) return { status: 403, body: { error: "You don't own this Blend." } };
  const now = deps.now();
  const counts = await env.DB.prepare(
    `SELECT SUM(CASE WHEN at > ? THEN 1 ELSE 0 END) AS hour,
            SUM(CASE WHEN slug = ? AND at > ? THEN 1 ELSE 0 END) AS pack
       FROM blend_downloads WHERE steam_id = ? AND at > ?`,
  )
    .bind(now - HOUR, slug, now - DAY, steamId, now - DAY)
    .first<{ hour: number | null; pack: number | null }>();
  if ((counts?.hour ?? 0) >= LINKS_PER_HOUR || (counts?.pack ?? 0) >= LINKS_PER_PACK_PER_DAY) {
    return { status: 429, body: { error: "Too many downloads. Try again later." } };
  }
  const head = await env.ASSET_LOCKED.head(`${ZIP_PREFIX}${slug}.zip`);
  if (!head) {
    await alert(env, deps, "blends", `A buyer asked for ${slug} and ${ZIP_PREFIX}${slug}.zip is not in mxb-private.`);
    return { status: 503, body: { error: "This download isn't ready yet. Try again later." } };
  }
  const row = await env.DB.prepare("INSERT INTO blend_downloads (order_id, steam_id, slug, at) VALUES (?, ?, ?, ?) RETURNING id")
    .bind(own.order_id, steamId, slug, now)
    .first<{ id: number }>();
  const exp = now + LINK_TTL_MS;
  const token = await sealLink({ d: row?.id ?? 0, o: own.order_id, s: steamId, slug, exp }, cfg.sessionKey);
  return { status: 200, body: { url: `${apiOrigin}/v1/web/blends/file/${token}`, expiresAt: exp } };
}

/** `GET /v1/web/blends/file/<token>`: the marked zip. */
async function serveFile(env: Env, deps: Deps, cfg: ShopConfig | null, steamId: string | null, token: string): Promise<Response> {
  const page = (status: number, text: string) =>
    new Response(text, { status, headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" } });
  if (!cfg) return page(503, "The shop is closed right now.");
  const now = deps.now();
  const claims = await openLink(token, cfg.sessionKey, now);
  if (!claims) return page(410, "This download link has expired. Start the download again from your account.");
  if (!steamId || steamId !== claims.s) return page(403, "Sign in with the Steam account that bought this.");
  const own = await owned(env, steamId, claims.slug);
  if (!own || own.order_id !== claims.o) return page(403, "You don't own this Blend.");
  const order = await orderBy(env, "id", claims.o);
  if (!order || order.status !== "paid") return page(403, "You don't own this Blend.");

  const key = `${ZIP_PREFIX}${claims.slug}.zip`;
  const head = await env.ASSET_LOCKED.head(key);
  if (!head) return page(503, "This download isn't ready yet.");
  const range = async (offset: number, length: number) => {
    if (length <= 0) return new Uint8Array(0);
    const obj = await env.ASSET_LOCKED.get(key, { range: { offset, length } });
    if (!obj) throw new Error(`${key} went missing`);
    return obj.body as ReadableStream<Uint8Array>;
  };
  const at = new Date(now);
  const marked = await markZip(
    { size: head.size, range },
    {
      comment: `mxbsecure Blend. Order ${order.ref}. Licensed to one person under https://mxbsecure.com/legal/blends-license`,
      name: "ORDER.txt",
      text: [
        `Order: ${order.ref}`,
        `Blend: ${order.name}`,
        `Date: ${at.toISOString().slice(0, 10)}`,
        "",
        "Licensed to the buyer only. Do not share or resell these files.",
        "Credit in your mod: Model by mxbsecure",
        "License: https://mxbsecure.com/legal/blends-license",
        "",
      ].join("\r\n"),
      at,
    },
  );
  if (!marked) {
    await alert(env, deps, "blends", `${key} isn't a zip this can read. Re-upload it.`);
    return page(503, "This download isn't ready yet.");
  }
  await env.DB.prepare("UPDATE blend_downloads SET served_at = COALESCE(served_at, ?) WHERE id = ?").bind(now, claims.d).run();
  await env.DB.prepare("UPDATE blend_orders SET first_download_at = ?, updated_at = ? WHERE id = ? AND first_download_at IS NULL")
    .bind(now, now, order.id)
    .run();
  console.log(JSON.stringify({ msg: "blends download", order: order.ref, slug: order.slug }));
  // On Workers a plain stream goes out chunked; a FixedLengthStream keeps the length, so the browser shows progress.
  let body: ReadableStream<Uint8Array> = marked.body;
  if (typeof FixedLengthStream === "function") {
    const fixed = new FixedLengthStream(marked.length);
    void marked.body.pipeTo(fixed.writable).catch((err) => console.error(JSON.stringify({ msg: "blends download stream", error: String(err) })));
    body = fixed.readable;
  }
  return new Response(body, {
    headers: {
      "content-type": "application/zip",
      "content-length": String(marked.length),
      "content-disposition": `attachment; filename="${claims.slug}-${order.ref}.zip"`,
      "cache-control": "no-store, private",
      "x-content-type-options": "nosniff",
    },
  });
}

/** `POST /v1/web/blends/billing`: Stripe's billing portal, for receipts and invoices. */
async function billingPortal(env: Env, deps: Deps, cfg: ShopConfig, steamId: string): Promise<Result> {
  const customer = await env.DB.prepare("SELECT customer_id FROM host_billing_customers WHERE steam_id = ?")
    .bind(steamId)
    .first<{ customer_id: string }>();
  if (!customer) return { status: 404, body: { error: "No purchases yet." } };
  const session = await stripe<{ url: string }>(cfg, deps, "POST", "/billing_portal/sessions", {
    customer: customer.customer_id,
    return_url: `${cfg.site}/account`,
    ...(env.STRIPE_PORTAL_CONFIG?.trim() ? { configuration: env.STRIPE_PORTAL_CONFIG.trim() } : {}),
  });
  if (!session.ok) return { status: 502, body: { error: "Billing couldn't be opened. Try again." } };
  return { status: 200, body: { url: session.body.url } };
}

/**
 * `POST /v1/web/blends/account/delete` `{ confirm: "DELETE" }`: the shop forgets this account.
 *
 * Entitlements and the download log go, so nothing can be downloaded again. Orders stay for the
 * tax and accounting records the law makes us keep, with the Steam account taken off them. The
 * Stripe customer is deleted too, unless a hosted server is still billed to it.
 */
export async function eraseShopAccount(env: Env, deps: Deps, steamId: string): Promise<{ cleared: string[]; kept: { what: string; why: string }[]; erasedAt: number }> {
  const now = deps.now();
  const cleared: string[] = [];
  const kept: { what: string; why: string }[] = [];
  const ent = await env.DB.prepare("DELETE FROM blend_entitlements WHERE steam_id = ?").bind(steamId).run();
  if (ent.meta.changes) cleared.push("blend_entitlements");
  const dl = await env.DB.prepare("DELETE FROM blend_downloads WHERE steam_id = ?").bind(steamId).run();
  if (dl.meta.changes) cleared.push("blend_downloads");
  const orders = await env.DB.prepare("UPDATE blend_orders SET steam_id = 'erased', customer_id = NULL, updated_at = ? WHERE steam_id = ?")
    .bind(now, steamId)
    .run();
  if (orders.meta.changes) kept.push({ what: "blend_orders", why: "Kept without your Steam account for tax and accounting records." });

  const customer = await env.DB.prepare("SELECT customer_id FROM host_billing_customers WHERE steam_id = ?")
    .bind(steamId)
    .first<{ customer_id: string }>();
  if (customer) {
    const hosting = await env.DB.prepare(
      "SELECT 1 AS h FROM host_billing WHERE steam_id = ? AND status IN ('pending', 'active', 'past_due', 'canceled') LIMIT 1",
    )
      .bind(steamId)
      .first();
    const key = env.STRIPE_SECRET_KEY?.trim();
    if (hosting) {
      kept.push({ what: "stripe_customer", why: "A hosted server is still billed to it." });
    } else if (key) {
      const gone = await stripe(
        { secretKey: key },
        deps,
        "DELETE",
        `/customers/${encodeURIComponent(customer.customer_id)}`,
      );
      if (gone.ok || gone.status === 404) {
        await env.DB.prepare("DELETE FROM host_billing_customers WHERE steam_id = ?").bind(steamId).run();
        cleared.push("stripe_customer");
      } else {
        await alert(env, deps, "blends", `Deleting Stripe customer ${customer.customer_id} for an erased account failed: ${gone.error}. Delete it in Stripe.`);
        kept.push({ what: "stripe_customer", why: "Stripe didn't answer. We'll delete it by hand." });
      }
    }
  }
  console.log(JSON.stringify({ msg: "blends account erased", cleared, kept: kept.map((k) => k.what) }));
  return { cleared, kept, erasedAt: now };
}

// ---- Webhook -------------------------------------------------------------------------------

interface StripeEvent {
  id: string;
  type: string;
  data: { object: Record<string, unknown> };
}

function str(v: unknown): string | null {
  if (typeof v === "string" && v) return v;
  if (v && typeof v === "object" && typeof (v as { id?: unknown }).id === "string") return (v as { id: string }).id;
  return null;
}

function obj(v: unknown): Record<string, unknown> {
  return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
}

function int(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? Math.round(v) : null;
}

async function grant(env: Env, order: OrderRow, now: number): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO blend_entitlements (steam_id, slug, order_id, granted_at) VALUES (?, ?, ?, ?)
     ON CONFLICT(steam_id, slug) DO UPDATE SET order_id = excluded.order_id, granted_at = excluded.granted_at, revoked_at = NULL, revoked_why = NULL`,
  )
    .bind(order.steam_id, order.slug, order.id, now)
    .run();
}

async function revoke(env: Env, order: OrderRow, why: string, now: number): Promise<void> {
  await env.DB.prepare("UPDATE blend_entitlements SET revoked_at = ?, revoked_why = ? WHERE steam_id = ? AND slug = ? AND order_id = ? AND revoked_at IS NULL")
    .bind(now, why, order.steam_id, order.slug, order.id)
    .run();
}

/** The order a charge or dispute is about: by PaymentIntent, else by the metadata we put on it. */
async function orderForPayment(env: Env, o: Record<string, unknown>): Promise<OrderRow | null> {
  const pi = str(o.payment_intent);
  if (pi) {
    const row = await orderBy(env, "payment_intent_id", pi);
    if (row) return row;
  }
  const id = str(obj(o.metadata).order_id);
  return id ? orderBy(env, "id", id) : null;
}

/**
 * One Stripe event, for the shop. Events that aren't the shop's are ignored, so this runs on
 * every event the endpoint receives. Each move is one conditional UPDATE, so two deliveries of
 * the same payment can't grant twice.
 */
export async function applyBlendEvent(env: Env, deps: Deps, event: StripeEvent): Promise<void> {
  const o = event.data?.object ?? {};
  const now = deps.now();
  switch (event.type) {
    case "checkout.session.completed":
    case "checkout.session.async_payment_succeeded": {
      if (o.mode !== "payment" || obj(o.metadata).kind !== "blend") return;
      const order = (str(o.id) && (await orderBy(env, "checkout_session_id", str(o.id)!))) || (await orderBy(env, "id", str(obj(o.metadata).order_id) ?? ""));
      if (!order) {
        await alert(env, deps, "blends", `Stripe reported Checkout ${str(o.id)} for a Blends order this side doesn't have.`);
        return;
      }
      const pi = str(o.payment_intent);
      if (o.payment_status !== "paid") {
        if (pi) await env.DB.prepare("UPDATE blend_orders SET payment_intent_id = COALESCE(payment_intent_id, ?), updated_at = ? WHERE id = ?").bind(pi, now, order.id).run();
        return;
      }
      const totals = obj(o.total_details);
      const moved = await env.DB.prepare(
        `UPDATE blend_orders SET status = 'paid', paid_at = ?, payment_intent_id = COALESCE(?, payment_intent_id),
                subtotal_cents = ?, tax_cents = ?, total_cents = ?, currency = COALESCE(?, currency), updated_at = ?
          WHERE id = ? AND status IN ('pending', 'expired')`,
      )
        .bind(now, pi, int(o.amount_subtotal), int(totals.amount_tax), int(o.amount_total), str(o.currency), now, order.id)
        .run();
      if (!moved.meta.changes) return;
      await grant(env, order, now);
      console.log(JSON.stringify({ msg: "blends paid", order: order.ref, slug: order.slug }));
      return;
    }
    case "checkout.session.expired":
    case "checkout.session.async_payment_failed": {
      if (obj(o.metadata).kind !== "blend" || !str(o.id)) return;
      await env.DB.prepare("UPDATE blend_orders SET status = 'expired', closed_at = ?, updated_at = ? WHERE checkout_session_id = ? AND status = 'pending'")
        .bind(now, now, str(o.id))
        .run();
      return;
    }
    case "charge.refunded": {
      const order = await orderForPayment(env, o);
      if (!order || order.status !== "paid") return;
      // Only a full refund takes the files away; a partial one is a goodwill credit.
      const amount = int(o.amount);
      const refunded = int(o.amount_refunded);
      if (o.refunded !== true && !(amount !== null && refunded !== null && refunded >= amount)) return;
      const moved = await env.DB.prepare("UPDATE blend_orders SET status = 'refunded', closed_at = ?, updated_at = ? WHERE id = ? AND status = 'paid'")
        .bind(now, now, order.id)
        .run();
      if (moved.meta.changes) await revoke(env, order, "refunded", now);
      return;
    }
    case "charge.dispute.created": {
      const order = await orderForPayment(env, o);
      if (!order || order.status !== "paid") return;
      const moved = await env.DB.prepare("UPDATE blend_orders SET status = 'disputed', closed_at = ?, updated_at = ? WHERE id = ? AND status = 'paid'")
        .bind(now, now, order.id)
        .run();
      if (!moved.meta.changes) return;
      await revoke(env, order, "disputed", now);
      await alert(env, deps, "blends", `Chargeback on Blends order ${order.ref} (${order.slug}). Downloads are off. Answer it in Stripe.`);
      return;
    }
    case "charge.dispute.closed": {
      if (o.status !== "won") return;
      const order = await orderForPayment(env, o);
      if (!order || order.status !== "disputed") return;
      const moved = await env.DB.prepare("UPDATE blend_orders SET status = 'paid', closed_at = NULL, updated_at = ? WHERE id = ? AND status = 'disputed'")
        .bind(now, order.id)
        .run();
      if (moved.meta.changes) await grant(env, order, now);
      return;
    }
    default:
      return;
  }
}

/** Pending orders whose Checkout is long gone (a missed `expired` event). From the cron tick. */
export async function blendShopTick(env: Env, deps: Deps): Promise<void> {
  const now = deps.now();
  await env.DB.prepare("UPDATE blend_orders SET status = 'expired', closed_at = ?, updated_at = ? WHERE status = 'pending' AND created_at < ?")
    .bind(now, now, now - DAY)
    .run();
}
