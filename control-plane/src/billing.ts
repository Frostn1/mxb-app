/**
 * Paid hosting: one Stripe subscription per hosted server (`hosting.ts`), billed by Creste LLC.
 *
 * Off unless all four of `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRICE_MXBSERVER`
 * and `STRIPE_PRICE_LEGACY` are set. Off, deploy is exactly what it was: invite-only and free.
 * On, invites still gate who may deploy, and a deploy:
 *
 *   1. checks there is room for the server (a free slot, a box on its way, or room under the cap),
 *   2. writes the server as `pending` (no slot, no box ordered) and opens Stripe Checkout,
 *   3. is placed only when Stripe's webhook says it is paid (`checkout.session.completed` with
 *      `payment_status: paid`, or `invoice.paid`), through the same placement a free deploy uses.
 *
 * A failed renewal (`invoice.payment_failed`) or an ended subscription starts a grace of
 * `GRACE_DAYS`. Unpaid at the end of it, the server is deleted through the same path the idle
 * sweep uses (its slot is freed) and the subscription is cancelled. Deleting a paid server any
 * other way (operator, idle) cancels its subscription too; an owner's delete stops it renewing
 * at the end of the paid month instead. Owners can also cancel (at period end) and resume.
 *
 * Workers have no Stripe SDK, so this calls Stripe's REST API with `fetch`, and checks the
 * webhook signature with Web Crypto.
 */

import { tokenMatches } from "./auth";
import { ALREADY_SEATED, PAID_FAILED_ERROR, alert, deleteServer, seatPaid, type Deps, type Result } from "./hosting";
import type { ServerType } from "./hostregions";

const DAY = 24 * 60 * 60 * 1000;
export const GRACE_DAYS = 3;
/** An unpaid Checkout is dropped after this long (Stripe's own session expiry is 24 hours). */
const PENDING_TTL_MS = DAY;
/** How old a webhook signature may be. Stripe's own libraries use the same 5 minutes. */
const SIGNATURE_TOLERANCE_S = 300;
const STRIPE_API = "https://api.stripe.com/v1";
export const SITE = "https://servers.mxbsecure.com";

/** USD cents a month per hosted server, as decided. The Stripe prices are made to match. */
export const PRICE_CENTS: Record<ServerType, number> = { mxbserver: 500, legacy: 800 };

export type BillingStatus = "pending" | "active" | "past_due" | "canceled" | "suspended" | "ended" | "expired";

export interface BillingConfig {
  secretKey: string;
  webhookSecret: string;
  prices: Record<ServerType, string>;
}

/** The config, or null when billing is off (any of the four missing). */
export function billingConfig(env: Env): BillingConfig | null {
  const secretKey = env.STRIPE_SECRET_KEY?.trim();
  const webhookSecret = env.STRIPE_WEBHOOK_SECRET?.trim();
  const mxbserver = env.STRIPE_PRICE_MXBSERVER?.trim();
  const legacy = env.STRIPE_PRICE_LEGACY?.trim();
  if (!secretKey || !webhookSecret || !mxbserver || !legacy) return null;
  return { secretKey, webhookSecret, prices: { mxbserver, legacy } };
}

export interface BillingRow {
  server_id: string;
  steam_id: string;
  type: ServerType;
  price_id: string;
  amount_cents: number;
  status: BillingStatus;
  customer_id: string;
  checkout_session_id: string | null;
  subscription_id: string | null;
  grace_until: number | null;
  activated_at: number | null;
  refund_id: string | null;
  refunded_at: number | null;
  created_at: number;
  updated_at: number;
}

async function billingRow(env: Env, serverId: string): Promise<BillingRow | null> {
  return env.DB.prepare("SELECT * FROM host_billing WHERE server_id = ?").bind(serverId).first<BillingRow>();
}

async function setRow(env: Env, deps: Deps, serverId: string, fields: Partial<BillingRow>): Promise<void> {
  const keys = Object.keys(fields) as (keyof BillingRow)[];
  if (!keys.length) return;
  await env.DB.prepare(`UPDATE host_billing SET ${keys.map((k) => `${k} = ?`).join(", ")}, updated_at = ? WHERE server_id = ?`)
    .bind(...keys.map((k) => fields[k] ?? null), deps.now(), serverId)
    .run();
}

// ---- Stripe REST ---------------------------------------------------------------------------

type StripeResult<T> = { ok: true; body: T } | { ok: false; status: number; error: string };

/** Stripe's form encoding, flattened: `{ a: { b: "c" } }` -> `a[b]=c`. */
export function formEncode(params: Record<string, unknown>, prefix = ""): string[] {
  const out: string[] = [];
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue;
    const name = prefix ? `${prefix}[${key}]` : key;
    if (typeof value === "object") out.push(...formEncode(value as Record<string, unknown>, name));
    else out.push(`${encodeURIComponent(name)}=${encodeURIComponent(String(value))}`);
  }
  return out;
}

async function stripe<T>(
  cfg: { secretKey: string },
  deps: Deps,
  method: "GET" | "POST" | "DELETE",
  path: string,
  params: Record<string, unknown> = {},
  idempotencyKey?: string,
): Promise<StripeResult<T>> {
  const body = formEncode(params).join("&");
  try {
    const res = await deps.fetch(`${STRIPE_API}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${cfg.secretKey}`,
        "Content-Type": "application/x-www-form-urlencoded",
        ...(idempotencyKey ? { "Idempotency-Key": idempotencyKey } : {}),
      },
      body: method === "GET" || !body ? undefined : body,
      signal: AbortSignal.timeout(15_000),
    });
    const parsed = (await res.json().catch(() => null)) as (T & { error?: { message?: string } }) | null;
    if (!res.ok) return { ok: false, status: res.status, error: parsed?.error?.message ?? `Stripe ${res.status}` };
    return { ok: true, body: parsed as T };
  } catch (err) {
    return { ok: false, status: 502, error: String(err).slice(0, 200) };
  }
}

/** The Stripe customer for a Steam account, made on first use. */
async function customerFor(env: Env, deps: Deps, cfg: BillingConfig, steamId: string): Promise<string | null> {
  const known = await env.DB.prepare("SELECT customer_id FROM host_billing_customers WHERE steam_id = ?")
    .bind(steamId)
    .first<{ customer_id: string }>();
  if (known) return known.customer_id;
  const made = await stripe<{ id: string }>(
    cfg,
    deps,
    "POST",
    "/customers",
    { description: `Steam ${steamId}`, metadata: { steam_id: steamId } },
    `mxb-customer-${steamId}`,
  );
  if (!made.ok) {
    console.error(JSON.stringify({ msg: "billing customer", status: made.status, error: made.error }));
    await alert(env, deps, "billing", `Stripe refused to create a customer (${made.status}): ${made.error}`);
    return null;
  }
  await env.DB.prepare(
    "INSERT INTO host_billing_customers (steam_id, customer_id, created_at) VALUES (?, ?, ?) ON CONFLICT(steam_id) DO NOTHING",
  )
    .bind(steamId, made.body.id, deps.now())
    .run();
  const row = await env.DB.prepare("SELECT customer_id FROM host_billing_customers WHERE steam_id = ?")
    .bind(steamId)
    .first<{ customer_id: string }>();
  return row?.customer_id ?? null;
}

async function createCheckout(
  deps: Deps,
  cfg: BillingConfig,
  input: { serverId: string; customerId: string; type: ServerType; name: string },
): Promise<StripeResult<{ id: string; url: string }>> {
  const back = (outcome: string) => `${SITE}/mine?checkout=${outcome}&server=${input.serverId}`;
  return stripe<{ id: string; url: string }>(cfg, deps, "POST", "/checkout/sessions", {
    mode: "subscription",
    customer: input.customerId,
    client_reference_id: input.serverId,
    line_items: { 0: { price: cfg.prices[input.type], quantity: 1 } },
    metadata: { server_id: input.serverId },
    subscription_data: { description: `MX Bikes server: ${input.name}`, metadata: { server_id: input.serverId } },
    success_url: back("done"),
    cancel_url: back("cancelled"),
  });
}

async function cancelSubscription(env: Env, deps: Deps, row: BillingRow, why: string): Promise<void> {
  if (!row.subscription_id) return;
  const key = env.STRIPE_SECRET_KEY?.trim();
  const done: StripeResult<unknown> = key
    ? await stripe({ secretKey: key }, deps, "DELETE", `/subscriptions/${encodeURIComponent(row.subscription_id)}`)
    : { ok: false, status: 0, error: "STRIPE_SECRET_KEY is not set" };
  // 404: already gone at Stripe, which is the outcome wanted.
  if (!done.ok && done.status !== 404) {
    await alert(env, deps, "billing", `Cancelling the subscription for server ${row.server_id} (${why}) failed: ${done.error}. Cancel it in Stripe.`);
  }
}

async function expireCheckout(env: Env, deps: Deps, row: BillingRow): Promise<void> {
  const key = env.STRIPE_SECRET_KEY?.trim();
  if (!row.checkout_session_id || !key) return;
  // Best effort: a session that already completed or expired refuses, and that is fine.
  await stripe({ secretKey: key }, deps, "POST", `/checkout/sessions/${encodeURIComponent(row.checkout_session_id)}/expire`);
}

// ---- Deploy --------------------------------------------------------------------------------

/**
 * The deploy hook: the server row is already written as `pending`. Opens Checkout for it and
 * returns the URL, or an error the deploy hands back (and then drops the row).
 */
export async function openCheckout(
  env: Env,
  deps: Deps,
  cfg: BillingConfig,
  input: { serverId: string; steamId: string; type: ServerType; name: string },
): Promise<{ url: string } | { error: string }> {
  const customerId = await customerFor(env, deps, cfg, input.steamId);
  if (!customerId) return { error: "Payment couldn't be started. Try again." };
  const session = await createCheckout(deps, cfg, { ...input, customerId });
  if (!session.ok) {
    console.error(JSON.stringify({ msg: "billing checkout", status: session.status, error: session.error }));
    await alert(env, deps, "billing", `Stripe refused the checkout (${session.status}): ${session.error}`);
    return { error: "Payment couldn't be started. Try again." };
  }
  const now = deps.now();
  await env.DB.prepare(
    `INSERT INTO host_billing (server_id, steam_id, type, price_id, amount_cents, status, customer_id, checkout_session_id, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?)`,
  )
    .bind(input.serverId, input.steamId, input.type, cfg.prices[input.type], PRICE_CENTS[input.type], customerId, session.body.id, now, now)
    .run();
  return { url: session.body.url };
}

/** Stops renewal but keeps the paid month: no refund, no proration. */
async function cancelAtPeriodEnd(env: Env, deps: Deps, row: BillingRow, why: string): Promise<void> {
  if (!row.subscription_id) return;
  const key = env.STRIPE_SECRET_KEY?.trim();
  const done: StripeResult<unknown> = key
    ? await stripe({ secretKey: key }, deps, "POST", `/subscriptions/${encodeURIComponent(row.subscription_id)}`, {
        cancel_at_period_end: true,
      })
    : { ok: false, status: 0, error: "STRIPE_SECRET_KEY is not set" };
  if (!done.ok && done.status !== 404) {
    await alert(env, deps, "billing", `Ending the subscription for server ${row.server_id} (${why}) failed: ${done.error}. Cancel it in Stripe.`);
  }
}

/**
 * Called from `deleteServer`: whatever deleted a paid server, its billing stops with it.
 * `periodEnd` (an owner's delete): an active subscription runs out its paid month instead of
 * ending now. Never a refund here; a server that failed was already refunded and is `ended`.
 */
export async function billingServerGone(env: Env, deps: Deps, serverId: string, opts: { periodEnd?: boolean } = {}): Promise<void> {
  const row = await billingRow(env, serverId);
  if (!row) return;
  if (row.status === "pending") {
    await setRow(env, deps, serverId, { status: "expired" });
    await expireCheckout(env, deps, row);
  } else if (row.status === "active" && opts.periodEnd) {
    await setRow(env, deps, serverId, { status: "ended", grace_until: null });
    await cancelAtPeriodEnd(env, deps, row, "server deleted");
  } else if (row.status === "active" || row.status === "past_due") {
    await setRow(env, deps, serverId, { status: "ended", grace_until: null });
    await cancelSubscription(env, deps, row, "server deleted");
  } else if (row.status === "canceled") {
    await setRow(env, deps, serverId, { status: "ended", grace_until: null });
  }
}

/** The idle sweep skips these: a server someone is paying for is not reclaimed for idling. */
export const PAYING_SQL = "SELECT server_id FROM host_billing WHERE status IN ('active', 'past_due', 'canceled')";

// ---- Webhook -------------------------------------------------------------------------------

async function hmacHex(secret: string, message: string): Promise<string> {
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(message)));
  return Array.from(mac, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Stripe's `Stripe-Signature: t=<unix>,v1=<hex>[,v1=<hex>]`: HMAC-SHA256 of `<t>.<raw body>`
 * under the endpoint's signing secret, and `t` within the tolerance of now.
 */
export async function verifyStripeSignature(
  payload: string,
  header: string | null,
  secret: string,
  nowMs: number,
  toleranceS = SIGNATURE_TOLERANCE_S,
): Promise<boolean> {
  if (!header || !secret) return false;
  let t: number | null = null;
  const v1: string[] = [];
  for (const part of header.split(",")) {
    const i = part.indexOf("=");
    if (i < 0) continue;
    const k = part.slice(0, i).trim();
    const v = part.slice(i + 1).trim();
    if (k === "t" && /^\d{1,12}$/.test(v)) t = Number(v);
    else if (k === "v1" && /^[0-9a-f]{64}$/.test(v)) v1.push(v);
  }
  if (t === null || !v1.length) return false;
  if (Math.abs(nowMs / 1000 - t) > toleranceS) return false;
  const expected = await hmacHex(secret, `${t}.${payload}`);
  let match = false;
  for (const sig of v1) match = tokenMatches(expected, sig) || match;
  return match;
}

/** For tests and fixtures: a header Stripe would send for this body. */
export async function signStripePayload(payload: string, secret: string, t: number): Promise<string> {
  return `t=${t},v1=${await hmacHex(secret, `${t}.${payload}`)}`;
}

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

function meta(v: unknown): Record<string, unknown> {
  return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
}

/** The subscription and server an invoice is for, in both the old and the 2025 invoice shape. */
function invoiceRefs(inv: Record<string, unknown>): { subscription: string | null; serverId: string | null } {
  const parent = meta(meta(inv.parent).subscription_details);
  const older = meta(inv.subscription_details);
  const subscription = str(inv.subscription) ?? str(parent.subscription);
  const serverId = str(meta(parent.metadata).server_id) ?? str(meta(older.metadata).server_id);
  return { subscription, serverId };
}

async function rowFor(env: Env, serverId: string | null, subscription: string | null): Promise<BillingRow | null> {
  if (serverId) {
    const row = await billingRow(env, serverId);
    if (row) return row;
  }
  if (subscription) {
    return env.DB.prepare("SELECT * FROM host_billing WHERE subscription_id = ?").bind(subscription).first<BillingRow>();
  }
  return null;
}

/** User-facing text for a paid server that could not be placed. The cause stays with operators. */
export const REFUNDED_ERROR = `${PAID_FAILED_ERROR} You've been refunded.`;
export const REFUND_PENDING_ERROR = `${PAID_FAILED_ERROR} Your refund is on its way.`;

type Invoice = {
  id?: string;
  payment_intent?: unknown;
  charge?: unknown;
};

/**
 * Refund the first payment of a subscription: its latest invoice's PaymentIntent or charge.
 * Older API versions put those on the invoice; newer ones (2025-03-31 on) only list them under
 * `/invoice_payments`. Returns the refund id, or why it could not be made.
 */
async function refundFirstPayment(
  deps: Deps,
  key: string,
  serverId: string,
  subscriptionId: string,
): Promise<{ id: string } | { error: string }> {
  const cfg = { secretKey: key };
  const sub = await stripe<{ latest_invoice?: Invoice | string | null }>(
    cfg,
    deps,
    "GET",
    `/subscriptions/${encodeURIComponent(subscriptionId)}?expand[]=latest_invoice`,
  );
  if (!sub.ok) return { error: `reading the subscription failed (${sub.status}): ${sub.error}` };
  const inv = sub.body.latest_invoice;
  if (!inv || typeof inv !== "object") return { error: "the subscription has no invoice" };
  let target: Record<string, string> | null = str(inv.payment_intent)
    ? { payment_intent: str(inv.payment_intent)! }
    : str(inv.charge)
      ? { charge: str(inv.charge)! }
      : null;
  if (!target && inv.id) {
    const paid = await stripe<{ data?: { status?: string; payment?: { payment_intent?: unknown; charge?: unknown } }[] }>(
      cfg,
      deps,
      "GET",
      `/invoice_payments?invoice=${encodeURIComponent(inv.id)}`,
    );
    if (!paid.ok) return { error: `reading the invoice payments failed (${paid.status}): ${paid.error}` };
    for (const p of paid.body.data ?? []) {
      if (p.status && p.status !== "paid") continue;
      const pi = str(p.payment?.payment_intent);
      const ch = str(p.payment?.charge);
      target = pi ? { payment_intent: pi } : ch ? { charge: ch } : null;
      if (target) break;
    }
  }
  if (!target) return { error: "no payment found on the latest invoice" };
  const refund = await stripe<{ id: string }>(
    cfg,
    deps,
    "POST",
    "/refunds",
    { ...target, reason: "requested_by_customer", metadata: { server_id: serverId } },
    `mxb-refund-${serverId}`,
  );
  if (!refund.ok) return { error: `the refund failed (${refund.status}): ${refund.error}` };
  return { id: refund.body.id };
}

/**
 * Paid: place the server. If it can no longer be placed, refund the first payment, stop
 * billing, and tell the owner in one short line.
 *
 * Stripe sends `checkout.session.completed` and `invoice.paid` for the same payment, and they
 * can arrive at once in separate invocations: the pending -> active move is one conditional
 * UPDATE, and only the invocation that made it goes on to place the server.
 */
async function activate(env: Env, deps: Deps, row: BillingRow, subscription: string | null): Promise<void> {
  const subId = subscription ?? row.subscription_id;
  if (row.status === "expired" && subId && !row.subscription_id) {
    // Paid at the moment it was dropped: nothing to put it on, so stop billing and say so.
    await setRow(env, deps, row.server_id, { subscription_id: subId });
    await cancelSubscription(env, deps, { ...row, subscription_id: subId }, "paid after it was dropped");
    await alert(env, deps, "billing", `Server ${row.server_id} was paid for after its Checkout was dropped. Its subscription was cancelled; refund the payment in Stripe.`);
    return;
  }
  const now = deps.now();
  const claimed =
    row.status === "pending"
      ? await env.DB.prepare(
          `UPDATE host_billing SET status = 'active', subscription_id = COALESCE(?, subscription_id), activated_at = ?,
                  grace_until = NULL, updated_at = ?
            WHERE server_id = ? AND status = 'pending'`,
        )
          .bind(subId, now, now, row.server_id)
          .run()
      : null;
  if (!claimed?.meta.changes) {
    // Another delivery for the same payment got here first (or it was never pending).
    if (subId) {
      await env.DB.prepare("UPDATE host_billing SET subscription_id = ?, updated_at = ? WHERE server_id = ? AND subscription_id IS NULL")
        .bind(subId, now, row.server_id)
        .run();
    }
    return;
  }
  const seated = await seatPaid(env, deps, row.server_id);
  if (seated === "ok" || seated === ALREADY_SEATED) {
    console.log(JSON.stringify({ msg: "billing active", server: row.server_id }));
    return;
  }
  await setRow(env, deps, row.server_id, { status: "ended" });
  const paidRow = { ...row, subscription_id: subId };
  const key = env.STRIPE_SECRET_KEY?.trim();
  const refund = !key
    ? { error: "STRIPE_SECRET_KEY is not set" }
    : subId
      ? await refundFirstPayment(deps, key, row.server_id, subId)
      : { error: "no subscription to refund" };
  if ("id" in refund) {
    await setRow(env, deps, row.server_id, { refund_id: refund.id, refunded_at: deps.now() });
  }
  await env.DB.prepare("UPDATE host_servers SET error = ? WHERE id = ? AND state = 'failed'")
    .bind("id" in refund ? REFUNDED_ERROR : REFUND_PENDING_ERROR, row.server_id)
    .run();
  await cancelSubscription(env, deps, paidRow, "not placed");
  console.log(JSON.stringify({ msg: "billing not placed", server: row.server_id, cause: seated, refunded: "id" in refund }));
  if ("error" in refund) {
    await alert(
      env,
      deps,
      "billing",
      `Server ${row.server_id} was paid for but could not be placed (${seated}). Refunding it failed: ${refund.error}. Refund the first payment in Stripe.`,
      { region: null },
    );
  }
}

/** Into grace, from active. Keeps an earlier deadline if one is already running. */
async function startGrace(env: Env, deps: Deps, row: BillingRow, status: "past_due" | "canceled"): Promise<void> {
  if (!["active", "past_due", "canceled"].includes(row.status)) return;
  const until = deps.now() + GRACE_DAYS * DAY;
  await setRow(env, deps, row.server_id, { status, grace_until: row.grace_until ? Math.min(row.grace_until, until) : until });
}

export async function applyEvent(env: Env, deps: Deps, event: StripeEvent): Promise<void> {
  const obj = event.data?.object ?? {};
  switch (event.type) {
    case "checkout.session.completed":
    case "checkout.session.async_payment_succeeded": {
      if (obj.mode !== "subscription") return;
      const row = await rowFor(env, str(meta(obj.metadata).server_id) ?? str(obj.client_reference_id), null);
      if (!row) return;
      const subscription = str(obj.subscription);
      if (obj.payment_status === "paid" || obj.payment_status === "no_payment_required") {
        await activate(env, deps, row, subscription);
      } else if (subscription && !row.subscription_id) {
        await setRow(env, deps, row.server_id, { subscription_id: subscription });
      }
      return;
    }
    case "checkout.session.expired": {
      const row = await rowFor(env, str(meta(obj.metadata).server_id) ?? str(obj.client_reference_id), null);
      if (!row || row.status !== "pending" || row.checkout_session_id !== str(obj.id)) return;
      await setRow(env, deps, row.server_id, { status: "expired" });
      await deleteServer(env, deps, row.server_id, "checkout expired");
      return;
    }
    case "invoice.paid": {
      const refs = invoiceRefs(obj);
      const row = await rowFor(env, refs.serverId, refs.subscription);
      if (!row) return;
      if (row.status === "pending") return activate(env, deps, row, refs.subscription);
      if (row.status === "past_due" || row.status === "canceled") {
        await setRow(env, deps, row.server_id, { status: "active", grace_until: null });
      }
      return;
    }
    case "invoice.payment_failed": {
      const refs = invoiceRefs(obj);
      const row = await rowFor(env, refs.serverId, refs.subscription);
      if (row && row.status !== "pending") await startGrace(env, deps, row, "past_due");
      return;
    }
    case "customer.subscription.updated": {
      const row = await rowFor(env, str(meta(obj.metadata).server_id), str(obj.id));
      if (!row) return;
      const status = obj.status;
      if ((status === "active" || status === "trialing") && row.status === "past_due") {
        await setRow(env, deps, row.server_id, { status: "active", grace_until: null });
      } else if (status === "past_due" || status === "unpaid") {
        await startGrace(env, deps, row, "past_due");
      } else if (status === "canceled") {
        await startGrace(env, deps, row, "canceled");
      }
      return;
    }
    case "customer.subscription.deleted": {
      const row = await rowFor(env, str(meta(obj.metadata).server_id), str(obj.id));
      if (!row) return;
      // The owner cancelled and the paid month ran out: the server goes now, no grace.
      if (meta(obj.cancellation_details).reason === "cancellation_requested" && ["active", "past_due", "canceled"].includes(row.status)) {
        await setRow(env, deps, row.server_id, { status: "ended", grace_until: null });
        await deleteServer(env, deps, row.server_id, "billing: cancelled by owner");
        return;
      }
      await startGrace(env, deps, row, "canceled");
      return;
    }
    default:
      return;
  }
}

/** `POST /v1/stripe/webhook`. Signature first, then once per event id. */
export async function stripeWebhook(request: Request, env: Env, deps: Deps): Promise<Response> {
  const reply = (status: number, body: unknown) =>
    new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
  const cfg = billingConfig(env);
  if (!cfg) {
    const missing = (["STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET", "STRIPE_PRICE_MXBSERVER", "STRIPE_PRICE_LEGACY"] as const).filter(
      (k) => !env[k]?.trim(),
    );
    return reply(503, { error: "billing is not configured", missing });
  }
  const raw = await request.text();
  if (raw.length > 512 * 1024) return reply(413, { error: "too large" });
  if (!(await verifyStripeSignature(raw, request.headers.get("Stripe-Signature"), cfg.webhookSecret, deps.now()))) {
    return reply(400, { error: "bad signature" });
  }
  let event: StripeEvent;
  try {
    event = JSON.parse(raw) as StripeEvent;
  } catch {
    return reply(400, { error: "bad body" });
  }
  if (typeof event?.id !== "string" || typeof event.type !== "string") return reply(400, { error: "bad event" });
  const claimed = await env.DB.prepare("INSERT INTO host_billing_events (id, type, received_at) VALUES (?, ?, ?) ON CONFLICT(id) DO NOTHING")
    .bind(event.id, event.type, deps.now())
    .run();
  if (!claimed.meta.changes) return reply(200, { received: true, duplicate: true });
  try {
    await applyEvent(env, deps, event);
  } catch (err) {
    // Let Stripe deliver it again.
    await env.DB.prepare("DELETE FROM host_billing_events WHERE id = ?").bind(event.id).run();
    console.error(JSON.stringify({ msg: "billing webhook", event: event.type, error: String(err) }));
    return reply(500, { error: "not handled" });
  }
  return reply(200, { received: true });
}

// ---- The cron tick -------------------------------------------------------------------------

/** Suspends servers whose grace ran out, and drops Checkouts nobody paid. */
export async function billingTick(env: Env, deps: Deps): Promise<void> {
  const now = deps.now();
  const due = await env.DB.prepare(
    "SELECT * FROM host_billing WHERE status IN ('past_due', 'canceled') AND grace_until IS NOT NULL AND grace_until <= ?",
  )
    .bind(now)
    .all<BillingRow>();
  for (const row of due.results) {
    await setRow(env, deps, row.server_id, { status: "suspended", grace_until: null });
    if (row.status === "past_due") await cancelSubscription(env, deps, row, "unpaid");
    await deleteServer(env, deps, row.server_id, "billing: unpaid");
    console.log(JSON.stringify({ msg: "billing suspended", server: row.server_id, was: row.status }));
  }
  const stale = await env.DB.prepare("SELECT * FROM host_billing WHERE status = 'pending' AND created_at < ?")
    .bind(now - PENDING_TTL_MS)
    .all<BillingRow>();
  for (const row of stale.results) {
    await setRow(env, deps, row.server_id, { status: "expired" });
    await expireCheckout(env, deps, row);
    await deleteServer(env, deps, row.server_id, "checkout not paid");
  }
}

// ---- The site ------------------------------------------------------------------------------

export function priceList(): { type: ServerType; cents: number; currency: "usd" }[] {
  return (Object.keys(PRICE_CENTS) as ServerType[]).map((type) => ({ type, cents: PRICE_CENTS[type], currency: "usd" }));
}

/** A subscription as Stripe returns it, trimmed to what the site shows. */
interface StripeSubscription {
  id: string;
  status?: string;
  cancel_at_period_end?: boolean;
  cancel_at?: number | null;
  /** Before API 2025-03-31 the period is on the subscription; after, on its items. */
  current_period_end?: number;
  items?: { data?: { current_period_end?: number }[] };
}

interface StripeInvoice {
  id: string;
  number?: string | null;
  created?: number;
  total?: number;
  amount_due?: number;
  currency?: string;
  status?: string | null;
  invoice_pdf?: string | null;
}

/** When a subscription bills next, and when it ends if it was cancelled. Epoch ms. */
export function subscriptionDates(sub: StripeSubscription): { nextBillAt: number | null; cancelAtPeriodEnd: boolean; endsAt: number | null } {
  const periodEnd = sub.current_period_end ?? sub.items?.data?.[0]?.current_period_end ?? null;
  const ends = sub.cancel_at ?? (sub.cancel_at_period_end ? periodEnd : null);
  const live = sub.status !== "canceled" && sub.status !== "incomplete_expired";
  return {
    nextBillAt: live && !ends && periodEnd ? periodEnd * 1000 : null,
    cancelAtPeriodEnd: live && Boolean(ends),
    endsAt: live && ends ? ends * 1000 : null,
  };
}

/** Only Stripe's own invoice PDFs are handed to the browser. */
function stripePdf(url: unknown): string | null {
  if (typeof url !== "string") return null;
  try {
    const u = new URL(url);
    return u.protocol === "https:" && (u.hostname === "pay.stripe.com" || u.hostname === "invoice.stripe.com") ? url : null;
  } catch {
    return null;
  }
}

/** This account's subscriptions and invoices, read live from Stripe (nothing is kept). */
async function stripeAccount(
  deps: Deps,
  cfg: BillingConfig,
  customerId: string,
): Promise<{ subs: Map<string, StripeSubscription>; invoices: Record<string, unknown>[] }> {
  const c = encodeURIComponent(customerId);
  const [subs, invoices] = await Promise.all([
    stripe<{ data?: StripeSubscription[] }>(cfg, deps, "GET", `/subscriptions?customer=${c}&status=all&limit=100`),
    stripe<{ data?: StripeInvoice[] }>(cfg, deps, "GET", `/invoices?customer=${c}&limit=24`),
  ]);
  return {
    subs: new Map((subs.ok ? (subs.body.data ?? []) : []).map((s) => [s.id, s])),
    invoices: (invoices.ok ? (invoices.body.data ?? []) : [])
      .filter((i) => i.status && i.status !== "draft")
      .map((i) => ({
        id: i.id,
        number: i.number ?? null,
        date: (i.created ?? 0) * 1000,
        amountCents: i.total ?? i.amount_due ?? 0,
        currency: i.currency ?? "usd",
        status: i.status,
        pdf: stripePdf(i.invoice_pdf),
      })),
  };
}

/** `GET /v1/web/hosting/billing`: prices, each of this account's paid servers, and its invoices. */
export async function myBilling(env: Env, deps: Deps, steamId: string): Promise<Result> {
  const cfg = billingConfig(env);
  const enabled = cfg !== null;
  const customer = await env.DB.prepare("SELECT customer_id FROM host_billing_customers WHERE steam_id = ?")
    .bind(steamId)
    .first<{ customer_id: string }>();
  const rows = await env.DB.prepare(
    `SELECT b.*, s.name AS name, s.region AS region FROM host_billing b LEFT JOIN host_servers s ON s.id = b.server_id
      WHERE b.steam_id = ? AND (b.status IN ('pending', 'active', 'past_due', 'canceled') OR (b.status = 'suspended' AND b.updated_at > ?))
      ORDER BY b.created_at`,
  )
    .bind(steamId, deps.now() - 30 * DAY)
    .all<BillingRow & { name: string | null; region: string | null }>();
  const live = cfg && customer ? await stripeAccount(deps, cfg, customer.customer_id) : null;
  return {
    status: 200,
    body: {
      enabled,
      prices: enabled ? priceList() : [],
      portal: enabled && Boolean(customer),
      servers: rows.results.map((r) => {
        const sub = r.subscription_id ? live?.subs.get(r.subscription_id) : undefined;
        return {
          serverId: r.server_id,
          name: r.name,
          type: r.type,
          region: r.region,
          status: r.status,
          amountCents: r.amount_cents,
          graceUntil: r.grace_until,
          ...(sub ? subscriptionDates(sub) : { nextBillAt: null, cancelAtPeriodEnd: false, endsAt: null }),
        };
      }),
      invoices: live?.invoices ?? [],
    },
  };
}

/**
 * `POST /v1/web/hosting/billing/servers/:id/cancel|resume`: stop renewing at the end of the
 * paid month, or take that back while the month is still running. No refund either way.
 */
export async function setRenewal(env: Env, deps: Deps, steamId: string, serverId: string, renew: boolean): Promise<Result> {
  const cfg = billingConfig(env);
  if (!cfg) return { status: 404, body: { error: "Billing is off." } };
  const row = await billingRow(env, serverId);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  if (row.status !== "active" || !row.subscription_id) return { status: 409, body: { error: "This server has no active plan." } };
  const sub = await stripe<StripeSubscription>(cfg, deps, "POST", `/subscriptions/${encodeURIComponent(row.subscription_id)}`, {
    cancel_at_period_end: !renew,
  });
  if (!sub.ok) {
    console.error(JSON.stringify({ msg: "billing renewal", status: sub.status, error: sub.error }));
    return { status: 502, body: { error: renew ? "Couldn't resume. Try again." : "Couldn't cancel. Try again." } };
  }
  console.log(JSON.stringify({ msg: renew ? "billing resumed" : "billing cancel at period end", server: serverId }));
  return { status: 200, body: { serverId, ...subscriptionDates(sub.body) } };
}

/** `POST /v1/web/hosting/billing/portal`: a Stripe Customer Portal session to manage billing. */
export async function portalLink(env: Env, deps: Deps, steamId: string): Promise<Result> {
  const cfg = billingConfig(env);
  if (!cfg) return { status: 404, body: { error: "Billing is off." } };
  const customer = await env.DB.prepare("SELECT customer_id FROM host_billing_customers WHERE steam_id = ?")
    .bind(steamId)
    .first<{ customer_id: string }>();
  if (!customer) return { status: 404, body: { error: "No billing yet." } };
  const session = await stripe<{ url: string }>(cfg, deps, "POST", "/billing_portal/sessions", {
    customer: customer.customer_id,
    return_url: `${SITE}/`,
    ...(env.STRIPE_PORTAL_CONFIG?.trim() ? { configuration: env.STRIPE_PORTAL_CONFIG.trim() } : {}),
  });
  if (!session.ok) return { status: 502, body: { error: "Billing couldn't be opened. Try again." } };
  return { status: 200, body: { url: session.body.url } };
}

/** `POST /v1/web/hosting/billing/servers/:id/checkout`: a fresh Checkout for a pending server. */
export async function resumeCheckout(env: Env, deps: Deps, steamId: string, serverId: string): Promise<Result> {
  const cfg = billingConfig(env);
  if (!cfg) return { status: 404, body: { error: "Billing is off." } };
  const row = await billingRow(env, serverId);
  if (!row || row.steam_id !== steamId) return { status: 404, body: { error: "No such server." } };
  if (row.status !== "pending") return { status: 409, body: { error: "This server is already paid." } };
  const server = await env.DB.prepare("SELECT name FROM host_servers WHERE id = ? AND state = 'pending'")
    .bind(serverId)
    .first<{ name: string }>();
  if (!server) return { status: 404, body: { error: "No such server." } };
  await expireCheckout(env, deps, row);
  const session = await createCheckout(deps, cfg, { serverId, customerId: row.customer_id, type: row.type, name: server.name });
  if (!session.ok) return { status: 502, body: { error: "Payment couldn't be started. Try again." } };
  await setRow(env, deps, serverId, { checkout_session_id: session.body.id });
  return { status: 200, body: { url: session.body.url } };
}

/** The billing routes under `/v1/web/hosting/billing`, or null for any other path. */
export async function billingWebRoute(env: Env, deps: Deps, steamId: string, method: string, path: string): Promise<Result | null> {
  if (!path.startsWith("/v1/web/hosting/billing")) return null;
  if (method === "GET" && path === "/v1/web/hosting/billing") return myBilling(env, deps, steamId);
  if (method === "POST" && path === "/v1/web/hosting/billing/portal") return portalLink(env, deps, steamId);
  const m = path.match(/^\/v1\/web\/hosting\/billing\/servers\/([0-9a-f-]{36})\/(checkout|cancel|resume)$/i);
  if (m && method === "POST") {
    const action = m[2].toLowerCase();
    if (action === "checkout") return resumeCheckout(env, deps, steamId, m[1]);
    return setRenewal(env, deps, steamId, m[1], action === "resume");
  }
  return { status: 404, body: { error: "no such endpoint" } };
}

// ---- Operators -----------------------------------------------------------------------------

/** Adds billing to the operator hosting view: each server's status and MRR, and the total. */
export async function withOperatorBilling(env: Env, result: Result): Promise<Result> {
  if (result.status !== 200 || !result.body || typeof result.body !== "object") return result;
  const body = result.body as { servers?: { id: string }[] } & Record<string, unknown>;
  const rows = await env.DB.prepare("SELECT * FROM host_billing").all<BillingRow>();
  const byServer = new Map(rows.results.map((r) => [r.server_id, r]));
  const billing = (r: BillingRow | undefined) => (r && (r.status === "active" || r.status === "past_due") ? r.amount_cents : 0);
  const count = (s: BillingStatus) => rows.results.filter((r) => r.status === s).length;
  return {
    ...result,
    body: {
      ...body,
      billing: {
        enabled: billingConfig(env) !== null,
        currency: "usd",
        mrrCents: rows.results.reduce((sum, r) => sum + billing(r), 0),
        active: count("active"),
        pastDue: count("past_due"),
        canceled: count("canceled"),
        pending: count("pending"),
        suspended: count("suspended"),
      },
      servers: (body.servers ?? []).map((s) => {
        const r = byServer.get(s.id);
        return { ...s, billingStatus: r?.status ?? null, graceUntil: r?.grace_until ?? null, mrrCents: billing(r) };
      }),
    },
  };
}
