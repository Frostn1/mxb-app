import { beforeEach, describe, expect, it, vi } from "vitest";
import { signStripePayload, stripeWebhook } from "../src/billing";
import { blendShopTick, EVIDENCE_TTL_MS, forgetCatalog, LINKS_PER_PACK_PER_DAY, sealLink, shopConfig } from "../src/blendshop";
import { defaultDeps } from "../src/hosting";
import { webRoutes } from "../src/web";
import { sealToken, SESSION_COOKIE } from "../src/websession";
import { crc32, endRecord, markZip, orderEntry, readEnd } from "../src/zipmark";
import { d1 } from "./d1sqlite";
import { fakeBucket } from "./modfakes";

// Test-mode style values only. No real Steam ids, keys or Stripe objects.
const KEY = "session-secret";
const WHSEC = "whsec_test_0123456789abcdef";
const RIDER = "76561190000000002";
const OTHER = "76561190000000003";
const SITE = "https://mxbsecure.com";
const API = "https://api.mxbsecure.com";
const SLUG = "wp-7448-aer-fork";

const CATALOG = {
  format: "mxb-blends/1",
  items: [
    { slug: SLUG, name: "WP XACT PRO 7448 AER Fork", priceCents: 2000 },
    { slug: "free-thing", name: "No price" },
  ],
};

/** A small stored zip with two files, made the plain way. */
function sampleZip(): Uint8Array {
  const at = new Date(Date.UTC(2026, 9, 12, 12, 0, 0));
  const a = orderEntry("Blend/fork.blend", "blend bytes", 0, at);
  const b = orderEntry("README.md", "read me", a.local.length, at);
  const cdOffset = a.local.length + b.local.length;
  const cd = new Uint8Array([...a.central, ...b.central]);
  const end = endRecord(2, cd.length, cdOffset, "");
  return new Uint8Array([...a.local, ...b.local, ...cd, ...end]);
}

/** The names in a zip's central directory, with each entry's data checked against its CRC. */
function listZip(bytes: Uint8Array): { names: string[]; comment: string; files: Record<string, string> } {
  const end = readEnd(bytes.subarray(bytes.length - Math.min(bytes.length, 22 + 0xffff)), bytes.length - Math.min(bytes.length, 22 + 0xffff))!;
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const commentLen = v.getUint16(end.eocdAt + 20, true);
  const comment = new TextDecoder().decode(bytes.subarray(end.eocdAt + 22, end.eocdAt + 22 + commentLen));
  const names: string[] = [];
  const files: Record<string, string> = {};
  let p = end.cdOffset;
  for (let i = 0; i < end.entries; i++) {
    expect(v.getUint32(p, true)).toBe(0x02014b50);
    const crc = v.getUint32(p + 16, true);
    const size = v.getUint32(p + 20, true);
    const nameLen = v.getUint16(p + 28, true);
    const extra = v.getUint16(p + 30, true);
    const cmt = v.getUint16(p + 32, true);
    const local = v.getUint32(p + 42, true);
    const name = new TextDecoder().decode(bytes.subarray(p + 46, p + 46 + nameLen));
    expect(v.getUint32(local, true)).toBe(0x04034b50);
    const lName = v.getUint16(local + 26, true);
    const lExtra = v.getUint16(local + 28, true);
    const data = bytes.subarray(local + 30 + lName + lExtra, local + 30 + lName + lExtra + size);
    expect(crc32(data)).toBe(crc);
    files[name] = new TextDecoder().decode(data);
    names.push(name);
    p += 46 + nameLen + extra + cmt;
  }
  expect(p).toBe(end.cdOffset + end.cdSize);
  return { names, comment, files };
}

function fakeStripe() {
  const calls: { url: string; method: string; body: URLSearchParams }[] = [];
  let sessions = 0;
  const f = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    calls.push({ url, method, body: new URLSearchParams(typeof init?.body === "string" ? init.body : "") });
    const json = (b: unknown, status = 200) => new Response(JSON.stringify(b), { status });
    if (url === `${SITE}/shop/blends.json`) return json(CATALOG);
    if (url === "https://api.stripe.com/v1/customers") {
      const steam = calls[calls.length - 1].body.get("metadata[steam_id]");
      return json({ id: steam === RIDER ? "cus_test_rider" : `cus_test_${steam}` });
    }
    if (url === "https://api.stripe.com/v1/checkout/sessions") {
      sessions++;
      return json({ id: `cs_test_${sessions}`, url: `https://checkout.stripe.com/c/pay/cs_test_${sessions}` });
    }
    if (url === "https://api.stripe.com/v1/billing_portal/sessions") return json({ url: "https://billing.stripe.com/p/session/test_portal" });
    if (url.startsWith("https://api.stripe.com/v1/customers/") && method === "DELETE") return json({ id: "cus_test_rider", deleted: true });
    return json({ error: { message: "unexpected" } }, 400);
  });
  return { fetch: f as unknown as typeof fetch, calls, stripeCalls: () => calls.filter((c) => c.url.startsWith("https://api.stripe.com/")) };
}

function deployment(vars: Record<string, string> = {}) {
  const bucket = fakeBucket();
  void bucket.put(`blends/${SLUG}.zip`, sampleZip());
  const e = {
    DB: d1(),
    ASSET_LOCKED: bucket,
    MXB_WEB_SESSION_KEY: KEY,
    MXB_SITE_ORIGIN: SITE,
    MXB_BLENDS_SHOP: "test",
    STRIPE_SECRET_KEY: "sk_test_fixture",
    STRIPE_WEBHOOK_SECRET: WHSEC,
    ...vars,
  } as unknown as Env;
  return { e, bucket, s: fakeStripe() };
}

async function cookie(steamId: string) {
  return `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId, name: "Rider", exp: Date.now() + 60_000 }, KEY)}`;
}

async function call(d: ReturnType<typeof deployment>, method: string, path: string, who: string | null, body?: unknown, headers: Record<string, string> = {}) {
  const h: Record<string, string> = { Origin: SITE, ...headers };
  if (who) h.Cookie = await cookie(who);
  if (body !== undefined) h["Content-Type"] = "application/json";
  const url = path.startsWith("http") ? path : `${API}${path}`;
  const r = new Request(url, { method, headers: h, body: body === undefined ? undefined : JSON.stringify(body) });
  return webRoutes(r, new URL(r.url), d.e, d.s.fetch);
}

async function deliver(d: ReturnType<typeof deployment>, event: unknown, secret = WHSEC) {
  const raw = JSON.stringify(event);
  const sig = await signStripePayload(raw, secret, Math.floor(Date.now() / 1000));
  const req = new Request(`${API}/v1/stripe/webhook`, { method: "POST", headers: { "Stripe-Signature": sig }, body: raw });
  return stripeWebhook(req, d.e, defaultDeps(d.e, d.s.fetch));
}

function completed(sessionId: string, orderId: string, id = "evt_test_paid") {
  return {
    id,
    type: "checkout.session.completed",
    data: {
      object: {
        id: sessionId,
        mode: "payment",
        payment_status: "paid",
        payment_intent: "pi_test_1",
        amount_subtotal: 2000,
        amount_total: 2160,
        currency: "usd",
        total_details: { amount_tax: 160 },
        metadata: { kind: "blend", order_id: orderId, slug: SLUG },
      },
    },
  };
}

async function buy(d: ReturnType<typeof deployment>, who = RIDER) {
  const res = await call(d, "POST", "/v1/web/blends/checkout", who, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12" });
  expect(res.status).toBe(200);
  const order = await d.e.DB.prepare("SELECT * FROM blend_orders WHERE steam_id = ? ORDER BY created_at DESC").bind(who).first<{ id: string; checkout_session_id: string; ref: string }>();
  return order!;
}

async function paid(d: ReturnType<typeof deployment>, who = RIDER) {
  const order = await buy(d, who);
  const r = await deliver(d, completed(order.checkout_session_id, order.id, `evt_test_paid_${order.id}`));
  expect(r.status).toBe(200);
  return order;
}

beforeEach(() => forgetCatalog());

describe("the switch", () => {
  it("is shut unless the mode and the key agree", () => {
    const base = { MXB_WEB_SESSION_KEY: KEY, STRIPE_WEBHOOK_SECRET: WHSEC };
    expect(shopConfig({ ...base, STRIPE_SECRET_KEY: "sk_test_x" } as unknown as Env)).toBeNull();
    expect(shopConfig({ ...base, STRIPE_SECRET_KEY: "sk_test_x", MXB_BLENDS_SHOP: "off" } as unknown as Env)).toBeNull();
    expect(shopConfig({ ...base, STRIPE_SECRET_KEY: "sk_live_x", MXB_BLENDS_SHOP: "test" } as unknown as Env)).toBeNull();
    expect(shopConfig({ ...base, STRIPE_SECRET_KEY: "sk_test_x", MXB_BLENDS_SHOP: "live" } as unknown as Env)).toBeNull();
    expect(shopConfig({ ...base, STRIPE_SECRET_KEY: "sk_test_x", MXB_BLENDS_SHOP: "test" } as unknown as Env)?.mode).toBe("test");
    expect(shopConfig({ ...base, STRIPE_SECRET_KEY: "rk_live_x", MXB_BLENDS_SHOP: "live" } as unknown as Env)?.mode).toBe("live");
  });

  it("says so to the page, and checkout refuses", async () => {
    const d = deployment({ MXB_BLENDS_SHOP: "off" });
    expect(await (await call(d, "GET", "/v1/web/blends", null)).json()).toEqual({ enabled: false, mode: null, owned: [] });
    const r = await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12" });
    expect(r.status).toBe(503);
    expect(d.s.stripeCalls()).toHaveLength(0);
  });
});

describe("checkout", () => {
  it("needs a sign-in, the agreement and the waiver", async () => {
    const d = deployment();
    expect((await call(d, "POST", "/v1/web/blends/checkout", null, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12" })).status).toBe(401);
    expect((await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, terms: "2026-10-12" })).status).toBe(400);
    expect((await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true })).status).toBe(400);
    expect((await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true, terms: "2026-10-12" })).status).toBe(400);
  });

  it("refuses a write from another site", async () => {
    const d = deployment();
    const r = await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12" }, { Origin: "https://evil.example.com" });
    expect(r.status).toBe(403);
  });

  it("sells only what the site prices", async () => {
    const d = deployment();
    expect((await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: "free-thing", agree: true, waiver: true, terms: "2026-10-12" })).status).toBe(404);
    expect((await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: "nope", agree: true, waiver: true, terms: "2026-10-12" })).status).toBe(404);
  });

  it("opens a Checkout at the site's price, with tax, an invoice and the order on it", async () => {
    const d = deployment();
    const res = await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12", priceCents: 1 });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { url: string; ref: string };
    expect(body.url).toMatch(/^https:\/\/checkout\.stripe\.com\//);
    expect(body.ref).toMatch(/^MXBB-[2-9A-Z]{8}$/);
    const sent = d.s.calls.find((c) => c.url === "https://api.stripe.com/v1/checkout/sessions")!;
    const p = sent.body;
    expect(p.get("mode")).toBe("payment");
    expect(p.get("customer")).toBe("cus_test_rider");
    expect(p.get("line_items[0][price_data][unit_amount]")).toBe("2000");
    expect(p.get("line_items[0][price_data][currency]")).toBe("usd");
    expect(p.get("line_items[0][price_data][tax_behavior]")).toBe("exclusive");
    expect(p.get("automatic_tax[enabled]")).toBe("true");
    expect(p.get("invoice_creation[enabled]")).toBe("true");
    expect(p.get("metadata[kind]")).toBe("blend");
    expect(p.get("metadata[slug]")).toBe(SLUG);
    expect(p.get("success_url")).toBe(`${SITE}/account?checkout=done&order=${body.ref}`);
    const row = await d.e.DB.prepare("SELECT * FROM blend_orders").first<{ status: string; price_cents: number; terms_version: string; id: string }>();
    expect(row).toMatchObject({ status: "pending", price_cents: 2000, terms_version: "2026-10-12" });
    expect((row as unknown as { waiver_at: number | null }).waiver_at).not.toBeNull();
    expect(p.get("metadata[order_id]")).toBe(row!.id);
    // Nothing is owned until Stripe says paid.
    expect(await (await call(d, "GET", "/v1/web/blends", RIDER)).json()).toMatchObject({ enabled: true, owned: [] });
  });
});

describe("the webhook", () => {
  it("refuses a bad signature and changes nothing", async () => {
    const d = deployment();
    const order = await buy(d);
    const r = await deliver(d, completed(order.checkout_session_id, order.id), "whsec_test_wrong");
    expect(r.status).toBe(400);
    expect((await d.e.DB.prepare("SELECT status FROM blend_orders").first<{ status: string }>())!.status).toBe("pending");
  });

  it("records the order and the entitlement once, however often Stripe says it", async () => {
    const d = deployment();
    const order = await buy(d);
    const ev = completed(order.checkout_session_id, order.id);
    expect((await deliver(d, ev)).status).toBe(200);
    expect(await (await deliver(d, ev)).json()).toEqual({ received: true, duplicate: true });
    // The same payment under another event id: still one grant.
    expect((await deliver(d, { ...ev, id: "evt_test_again", type: "checkout.session.async_payment_succeeded" })).status).toBe(200);
    const row = await d.e.DB.prepare("SELECT * FROM blend_orders").first<Record<string, unknown>>();
    expect(row).toMatchObject({ status: "paid", subtotal_cents: 2000, tax_cents: 160, total_cents: 2160, payment_intent_id: "pi_test_1" });
    const ents = await d.e.DB.prepare("SELECT * FROM blend_entitlements").all();
    expect(ents.results).toHaveLength(1);
    expect(await (await call(d, "GET", "/v1/web/blends", RIDER)).json()).toMatchObject({ owned: [SLUG] });
    // And it can't be bought twice.
    expect((await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12" })).status).toBe(409);
  });

  it("works with hosting billing off", async () => {
    const d = deployment();
    expect(d.e.STRIPE_PRICE_MXBSERVER).toBeUndefined();
    const order = await buy(d);
    expect((await deliver(d, completed(order.checkout_session_id, order.id))).status).toBe(200);
  });

  it("an unpaid or expired Checkout grants nothing", async () => {
    const d = deployment();
    const order = await buy(d);
    const ev = completed(order.checkout_session_id, order.id, "evt_test_unpaid");
    ev.data.object.payment_status = "unpaid";
    await deliver(d, ev);
    await deliver(d, { id: "evt_test_exp", type: "checkout.session.expired", data: { object: { id: order.checkout_session_id, metadata: { kind: "blend" } } } });
    expect((await d.e.DB.prepare("SELECT status FROM blend_orders").first<{ status: string }>())!.status).toBe("expired");
    expect((await d.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_entitlements").first<{ n: number }>())!.n).toBe(0);
  });

  it("a full refund takes the download away; a partial one doesn't", async () => {
    const d = deployment();
    await paid(d);
    await deliver(d, { id: "evt_test_part", type: "charge.refunded", data: { object: { payment_intent: "pi_test_1", amount: 2160, amount_refunded: 500, refunded: false } } });
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(200);
    await deliver(d, { id: "evt_test_full", type: "charge.refunded", data: { object: { payment_intent: "pi_test_1", amount: 2160, amount_refunded: 2160, refunded: true } } });
    expect((await d.e.DB.prepare("SELECT status FROM blend_orders").first<{ status: string }>())!.status).toBe("refunded");
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(403);
  });

  it("a chargeback stops downloads and raises an alert; a won dispute gives them back", async () => {
    const d = deployment();
    await paid(d);
    await deliver(d, { id: "evt_test_dp", type: "charge.dispute.created", data: { object: { payment_intent: "pi_test_1", status: "needs_response" } } });
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(403);
    const alert = await d.e.DB.prepare("SELECT message FROM host_alerts WHERE kind = 'blends'").first<{ message: string }>();
    expect(alert!.message).toMatch(/Chargeback/);
    await deliver(d, { id: "evt_test_dw", type: "charge.dispute.closed", data: { object: { payment_intent: "pi_test_1", status: "won" } } });
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(200);
  });
});

describe("downloads", () => {
  it("are only for the buyer", async () => {
    const d = deployment();
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(403);
    await paid(d);
    expect((await call(d, "POST", "/v1/web/blends/download", OTHER, { slug: SLUG })).status).toBe(403);
  });

  it("hand out a short-lived link to a zip marked with the order", async () => {
    const d = deployment();
    const order = await paid(d);
    const res = await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG });
    const { url, expiresAt } = (await res.json()) as { url: string; expiresAt: number };
    expect(url.startsWith(`${API}/v1/web/blends/file/`)).toBe(true);
    expect(expiresAt - Date.now()).toBeLessThanOrEqual(10 * 60 * 1000);

    // Someone else's browser, or no sign-in: refused.
    expect((await call(d, "GET", url, OTHER)).status).toBe(403);
    expect((await call(d, "GET", url, null)).status).toBe(403);

    const file = await call(d, "GET", url, RIDER);
    expect(file.status).toBe(200);
    expect(file.headers.get("content-type")).toBe("application/zip");
    expect(file.headers.get("content-disposition")).toContain(order.ref);
    const bytes = new Uint8Array(await file.arrayBuffer());
    expect(bytes.length).toBe(Number(file.headers.get("content-length")));
    const zip = listZip(bytes);
    expect(zip.names).toEqual(["Blend/fork.blend", "README.md", "ORDER.txt"]);
    expect(zip.files["Blend/fork.blend"]).toBe("blend bytes");
    expect(zip.files["ORDER.txt"]).toContain(`Order: ${order.ref}`);
    expect(zip.comment).toContain(order.ref);
    // The stored zip is untouched.
    expect(d.bucket.objects.get(`blends/${SLUG}.zip`)!.bytes).toEqual(sampleZip());
    const row = await d.e.DB.prepare("SELECT first_download_at FROM blend_orders").first<{ first_download_at: number | null }>();
    expect(row!.first_download_at).not.toBeNull();
  });

  it("refuse an expired or forged link", async () => {
    const d = deployment();
    const order = await paid(d);
    const old = await sealLink({ d: 1, o: order.id, s: RIDER, slug: SLUG, exp: Date.now() - 1 }, KEY);
    expect((await call(d, "GET", `/v1/web/blends/file/${old}`, RIDER)).status).toBe(410);
    const forged = await sealLink({ d: 1, o: order.id, s: RIDER, slug: SLUG, exp: Date.now() + 60_000 }, "another-key");
    expect((await call(d, "GET", `/v1/web/blends/file/${forged}`, RIDER)).status).toBe(410);
  });

  it("are rate limited", async () => {
    const d = deployment();
    await paid(d);
    for (let i = 0; i < LINKS_PER_PACK_PER_DAY; i++) expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(200);
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(429);
  });

  it("say so when the zip isn't uploaded", async () => {
    const d = deployment();
    await paid(d);
    d.bucket.objects.clear();
    expect((await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG })).status).toBe(503);
  });
});

describe("the account", () => {
  it("lists paid orders", async () => {
    const d = deployment();
    const order = await paid(d);
    await buy(d, OTHER);
    const { orders } = (await (await call(d, "GET", "/v1/web/blends/orders", RIDER)).json()) as { orders: Record<string, unknown>[] };
    expect(orders).toHaveLength(1);
    expect(orders[0]).toMatchObject({ ref: order.ref, slug: SLUG, status: "paid", totalCents: 2160, taxCents: 160, downloadable: true });
  });

  it("opens the billing portal", async () => {
    const d = deployment();
    await paid(d);
    const r = await call(d, "POST", "/v1/web/blends/billing", RIDER, {});
    expect(await r.json()).toEqual({ url: "https://billing.stripe.com/p/session/test_portal" });
    const sent = d.s.calls.find((c) => c.url.endsWith("/billing_portal/sessions"))!;
    expect(sent.body.get("return_url")).toBe(`${SITE}/account`);
  });

  it("is deleted on request: downloads gone, orders kept without the Steam id, Stripe customer deleted, signed out", async () => {
    const d = deployment();
    await paid(d);
    await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG });
    expect((await call(d, "POST", "/v1/web/blends/account/delete", RIDER, {})).status).toBe(400);
    const r = await call(d, "POST", "/v1/web/blends/account/delete", RIDER, { confirm: "DELETE" });
    expect(r.status).toBe(200);
    const body = (await r.json()) as { cleared: string[]; kept: { what: string }[] };
    expect(body.cleared).toEqual(expect.arrayContaining(["blend_entitlements", "blend_downloads", "stripe_customer"]));
    expect(body.kept.map((k) => k.what)).toEqual(["blend_orders"]);
    expect(r.headers.get("set-cookie")).toContain(`${SESSION_COOKIE}=;`);
    expect((await d.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_orders WHERE steam_id = ?").bind(RIDER).first<{ n: number }>())!.n).toBe(0);
    expect((await d.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_orders WHERE steam_id = 'erased'").first<{ n: number }>())!.n).toBe(1);
    expect(d.s.calls.some((c) => c.method === "DELETE" && c.url.endsWith("/customers/cus_test_rider"))).toBe(true);
    expect(await (await call(d, "GET", "/v1/web/blends", RIDER)).json()).toMatchObject({ owned: [] });
  });
});

describe("zipmark", () => {
  it("leaves a zip it can't read alone", async () => {
    const bytes = new TextEncoder().encode("not a zip at all, just some text that is long enough");
    const out = await markZip({ size: bytes.length, range: async (o, l) => bytes.subarray(o, o + l) }, { comment: "x", name: "ORDER.txt", text: "x", at: new Date() });
    expect(out).toBeNull();
  });

  it("replaces an old comment", async () => {
    const zip = sampleZip();
    const first = await markZip({ size: zip.length, range: async (o, l) => zip.subarray(o, o + l) }, { comment: "first", name: "A.txt", text: "a", at: new Date() });
    const once = new Uint8Array(await new Response(first!.body).arrayBuffer());
    const second = await markZip({ size: once.length, range: async (o, l) => once.subarray(o, o + l) }, { comment: "second", name: "B.txt", text: "b", at: new Date() });
    const twice = listZip(new Uint8Array(await new Response(second!.body).arrayBuffer()));
    expect(twice.comment).toBe("second");
    expect(twice.names).toEqual(["Blend/fork.blend", "README.md", "A.txt", "B.txt"]);
  });
});

describe("dispute records", () => {
  const ADMIN = "76561190000000009";
  const who = { "CF-Connecting-IP": "203.0.113.7", "User-Agent": "TestBrowser/1.0" };

  it("keep the address, browser, account and time of each checkout, link and download", async () => {
    const d = deployment({ ADMIN_STEAM_IDS: ADMIN });
    const res = await call(d, "POST", "/v1/web/blends/checkout", RIDER, { slug: SLUG, agree: true, waiver: true, terms: "2026-10-12" }, who);
    const { ref } = (await res.json()) as { ref: string };
    const order = (await d.e.DB.prepare("SELECT * FROM blend_orders").first<{ id: string; checkout_session_id: string }>())!;
    await deliver(d, completed(order.checkout_session_id, order.id));
    const { url } = (await (await call(d, "POST", "/v1/web/blends/download", RIDER, { slug: SLUG }, who)).json()) as { url: string };
    await (await call(d, "GET", url, RIDER, undefined, who)).arrayBuffer();
    const rows = await d.e.DB.prepare("SELECT * FROM blend_evidence ORDER BY id").all<Record<string, unknown>>();
    expect(rows.results.map((r) => r.kind)).toEqual(["checkout", "download_link", "download"]);
    for (const r of rows.results) expect(r).toMatchObject({ order_id: order.id, steam_id: RIDER, ip: "203.0.113.7", user_agent: "TestBrowser/1.0" });

    // The evidence pack: admins only.
    expect((await call(d, "GET", `/v1/web/blends/admin/evidence/${ref}`, RIDER)).status).toBe(403);
    const pack = (await (await call(d, "GET", `/v1/web/blends/admin/evidence/${ref}`, ADMIN)).json()) as Record<string, any>;
    expect(pack.order).toMatchObject({ ref, status: "paid", steamId: RIDER, stripePaymentIntent: "pi_test_1" });
    expect(pack.agreement.termsVersion).toBe("2026-10-12");
    expect(pack.agreement.immediateAccessWaiverAt).toBeTruthy();
    expect(pack.downloads).toHaveLength(1);
    expect(pack.activity).toHaveLength(3);
    expect((await call(d, "GET", "/v1/web/blends/admin/evidence/MXBB-NOPE2345", ADMIN)).status).toBe(404);
  });

  it("are deleted after two years", async () => {
    const d = deployment();
    await paid(d);
    const old = Date.now() - EVIDENCE_TTL_MS - 1000;
    await d.e.DB.prepare("UPDATE blend_evidence SET at = ?").bind(old).run();
    await blendShopTick(d.e, defaultDeps(d.e, d.s.fetch));
    expect((await d.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_evidence").first<{ n: number }>())!.n).toBe(0);
  });

  it("go with the account, except while a dispute is open", async () => {
    const d = deployment();
    await paid(d);
    await deliver(d, { id: "evt_test_dp2", type: "charge.dispute.created", data: { object: { payment_intent: "pi_test_1" } } });
    const r = (await (await call(d, "POST", "/v1/web/blends/account/delete", RIDER, { confirm: "DELETE" })).json()) as { kept: { what: string }[] };
    expect(r.kept.map((k) => k.what)).toContain("blend_evidence");
    expect((await d.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_evidence").first<{ n: number }>())!.n).toBeGreaterThan(0);

    const e = deployment();
    await paid(e);
    await call(e, "POST", "/v1/web/blends/account/delete", RIDER, { confirm: "DELETE" });
    expect((await e.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_evidence").first<{ n: number }>())!.n).toBe(0);
  });
});
