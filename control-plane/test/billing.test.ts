import { describe, expect, it, vi } from "vitest";
import { GRACE_DAYS, REFUNDED_ERROR, REFUND_PENDING_ERROR, billingTick, signStripePayload, stripeWebhook, verifyStripeSignature } from "../src/billing";
import {
  ALREADY_SEATED,
  addTrack,
  claimInvite,
  deleteServer,
  deploy,
  enrollBox,
  hostingTick,
  mintInvite,
  myHosting,
  operatorView,
  ownerDelete,
  seatPaid,
  type Deps,
} from "../src/hosting";
import { hostingWebRoutes } from "../src/hostingroutes";
import type { OvhClient } from "../src/ovh";
import { d1 } from "./d1sqlite";

// Test-mode style values only. No real Steam ids, keys or Stripe objects.
const BOSS = "76561190000000001";
const RIDER = "76561190000000002";
const OTHER = "76561190000000003";
const WHSEC = "whsec_test_0123456789abcdef";
const DAY = 24 * 60 * 60 * 1000;
const PERIOD_END_S = Math.floor(Date.parse("2026-11-07T10:00:00Z") / 1000);

const BILLING = {
  STRIPE_SECRET_KEY: "sk_test_fixture",
  STRIPE_WEBHOOK_SECRET: WHSEC,
  STRIPE_PRICE_MXBSERVER: "price_test_mxbserver",
  STRIPE_PRICE_LEGACY: "price_test_legacy",
};

function env(vars: Record<string, string> = {}): Env {
  return {
    DB: d1(),
    ADMIN_STEAM_IDS: BOSS,
    MXB_HOST_SPEND_CAP_USD: "30",
    MXB_HOST_BOX_PRICE_USD: "5.85",
    MXB_HOST_MAX_BOXES: "4",
    MXB_HOST_LEGACY_MAX_BOXES: "1",
    MXB_HOST_SLOTS_NATIVE: "4",
    MXB_HOST_SLOTS_LEGACY: "2",
    MXB_HOST_SSH_PUBLIC_KEY: "ssh-ed25519 AAAA test",
    MXB_GH_DISPATCH_TOKEN: "gh-token",
    MXB_BOX_ENROLL_KEY: "k".repeat(40),
    ...vars,
  } as unknown as Env;
}

function fakeOvh() {
  let next = 100;
  const delivered = new Map<number, string>();
  const ovh = {
    orderVps: vi.fn(async () => ({ orderId: ++next, price: 8.1, currency: "USD" })),
    deliveredService: vi.fn(async (id: number) => delivered.get(id) ?? null),
    ipv4: vi.fn(async () => "51.81.10.18"),
    renewsAt: vi.fn(async () => Date.parse("2026-11-07")),
    rebuild: vi.fn(async () => undefined),
    busy: vi.fn(async () => false),
    vps: vi.fn(async () => ({ state: "running" })),
  };
  return { ovh, deliver: (orderId: number, svc: string) => delivered.set(orderId, svc) };
}

/** Stripe in test mode, plus the slots and GitHub. Records every call. */
function fakeFetch(opts: { refundFails?: boolean } = {}) {
  const calls: { url: string; method: string; body: string }[] = [];
  let sessions = 0;
  const stripeState = { cancelAtPeriodEnd: false };
  // The 2025 shape: the period end is on the subscription item.
  const subscription = () => ({
    id: "sub_test_1",
    status: "active",
    cancel_at_period_end: stripeState.cancelAtPeriodEnd,
    cancel_at: stripeState.cancelAtPeriodEnd ? PERIOD_END_S : null,
    items: { data: [{ current_period_end: PERIOD_END_S }] },
  });
  const f = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    calls.push({ url, method, body: typeof init?.body === "string" ? init.body : "" });
    const json = (b: unknown, status = 200) => new Response(JSON.stringify(b), { status });
    if (url === "https://api.stripe.com/v1/customers") return json({ id: "cus_test_rider" });
    if (url === "https://api.stripe.com/v1/checkout/sessions") {
      sessions++;
      return json({ id: `cs_test_${sessions}`, url: `https://checkout.stripe.com/c/pay/cs_test_${sessions}` });
    }
    if (url.startsWith("https://api.stripe.com/v1/checkout/sessions/")) return json({ status: "expired" });
    if (url.startsWith("https://api.stripe.com/v1/subscriptions/") && method === "GET") {
      return json({ id: "sub_test_1", latest_invoice: { id: "in_test_1", payment_intent: "pi_test_1" } });
    }
    if (url.startsWith("https://api.stripe.com/v1/subscriptions/") && method === "POST") {
      const flag = new URLSearchParams(typeof init?.body === "string" ? init.body : "").get("cancel_at_period_end");
      if (flag) stripeState.cancelAtPeriodEnd = flag === "true";
      return json(subscription());
    }
    if (url.startsWith("https://api.stripe.com/v1/subscriptions/")) return json({ status: "canceled" });
    if (url.startsWith("https://api.stripe.com/v1/subscriptions?customer=cus_test_rider")) return json({ data: [subscription()] });
    if (url.startsWith("https://api.stripe.com/v1/invoices?customer=cus_test_rider")) {
      return json({
        data: [
          { id: "in_test_2", number: "TEST-0002", created: PERIOD_END_S - 30 * 86400, total: 500, currency: "usd", status: "paid", invoice_pdf: "https://pay.stripe.com/invoice/acct_test/in_test_2/pdf" },
          { id: "in_test_3", created: PERIOD_END_S, total: 500, currency: "usd", status: "draft", invoice_pdf: null },
          { id: "in_test_4", created: PERIOD_END_S - 60 * 86400, total: 500, currency: "usd", status: "open", invoice_pdf: "https://evil.example.com/pdf" },
        ],
      });
    }
    if (url === "https://api.stripe.com/v1/refunds") {
      return opts.refundFails ? json({ error: { message: "test refund refused" } }, 400) : json({ id: "re_test_1" });
    }
    if (url === "https://api.stripe.com/v1/billing_portal/sessions") return json({ url: "https://billing.stripe.com/p/session/test_portal" });
    if (url.startsWith("https://api.stripe.com/")) return json({ error: { message: "unexpected" } }, 400);
    if (url.startsWith("https://api.github.com/")) return new Response(null, { status: 204 });
    if (url.endsWith("/v1/riders")) return json({ riders: [] });
    return json({ ok: true });
  });
  const stripeCalls = () => calls.filter((c) => c.url.startsWith("https://api.stripe.com/"));
  return { fetch: f as unknown as typeof fetch, calls, stripeCalls };
}

function setup(vars: Record<string, string> = BILLING, opts: { refundFails?: boolean } = {}) {
  const e = env(vars);
  const { ovh, deliver } = fakeOvh();
  const f = fakeFetch(opts);
  const clock = { t: Date.parse("2026-10-07T10:00:00Z") };
  const d: Deps = { fetch: f.fetch, now: () => clock.t, ovh: ovh as unknown as OvhClient };
  return { e, d, f, clock, ovh, deliver };
}

async function invited(e: Env, d: Deps, steamId: string, quota = 1): Promise<void> {
  const minted = await mintInvite(e, d, BOSS, { steamId, quota });
  await claimInvite(e, d, steamId, { code: (minted.body as { code: string }).code });
}

/** A webhook delivery as Stripe sends it: the raw body, signed with the endpoint secret. */
async function deliver(e: Env, d: Deps, event: { id: string; type: string; data: { object: unknown } }, secret = WHSEC) {
  const raw = JSON.stringify({ object: "event", livemode: false, api_version: "2025-09-30.clover", ...event });
  const sig = await signStripePayload(raw, secret, Math.floor(d.now() / 1000));
  return stripeWebhook(
    new Request("https://api.mxbsecure.com/v1/stripe/webhook", { method: "POST", headers: { "Stripe-Signature": sig }, body: raw }),
    e,
    d,
  );
}

// Fixtures in Stripe's shapes (trimmed to the fields read).
const checkoutCompleted = (serverId: string, n = 1) => ({
  id: `evt_test_checkout_${serverId.slice(0, 8)}_${n}`,
  type: "checkout.session.completed",
  data: {
    object: {
      id: `cs_test_${n}`,
      object: "checkout.session",
      mode: "subscription",
      payment_status: "paid",
      status: "complete",
      client_reference_id: serverId,
      customer: "cus_test_rider",
      subscription: "sub_test_1",
      metadata: { server_id: serverId },
    },
  },
});
const invoice = (type: string, serverId: string, n: number) => ({
  id: `evt_test_inv_${n}`,
  type,
  data: {
    object: {
      id: `in_test_${n}`,
      object: "invoice",
      customer: "cus_test_rider",
      parent: { type: "subscription_details", subscription_details: { subscription: "sub_test_1", metadata: { server_id: serverId } } },
    },
  },
});

async function pendingDeploy(s: ReturnType<typeof setup>, type = "mxbserver") {
  await invited(s.e, s.d, RIDER);
  const r = await deploy(s.e, s.d, RIDER, { name: "Paid", type, region: "us-east" });
  expect(r.status).toBe(201);
  const body = r.body as { server: { id: string; state: string }; checkoutUrl: string };
  return { id: body.server.id, body };
}

/** Pay, then bring the ordered box all the way to ready so the server holds a slot. */
async function activeOnSlot(s: ReturnType<typeof setup>) {
  await addTrack(s.e, s.d, { pool: "native", name: "Club MX", url: "https://example.com/c.pkz", sha256: "a".repeat(64) });
  const { id } = await pendingDeploy(s);
  expect((await deliver(s.e, s.d, checkoutCompleted(id))).status).toBe(200);
  s.deliver(101, "vps-abc.vps.ovh.us");
  await hostingTick(s.e, s.d);
  s.clock.t += 3 * 60 * 1000;
  await hostingTick(s.e, s.d);
  const boxId = ((await operatorView(s.e)).body as { boxes: { id: string }[] }).boxes[0].id;
  await enrollBox(s.e, s.d, boxId, {
    slots: [1, 2, 3, 4].map((i) => ({ index: i, gamePort: 54209 + i, token: `cp-s${i}.${"f".repeat(64)}` })),
  });
  const view = (await myHosting(s.e, RIDER, false)).body as { servers: { id: string; state: string }[] };
  expect(view.servers[0]).toMatchObject({ id, state: "ready" });
  return { id, boxId };
}

async function billingOf(e: Env, serverId: string) {
  return e.DB.prepare("SELECT status, grace_until, subscription_id FROM host_billing WHERE server_id = ?")
    .bind(serverId)
    .first<{ status: string; grace_until: number | null; subscription_id: string | null }>();
}

async function usedSlots(e: Env): Promise<number> {
  return (await e.DB.prepare("SELECT COUNT(*) AS n FROM host_slots WHERE server_id IS NOT NULL").first<{ n: number }>())!.n;
}

describe("webhook signature", () => {
  const body = '{"id":"evt_test_1","type":"invoice.paid"}';
  const now = Date.parse("2026-10-07T10:00:00Z");
  const t = Math.floor(now / 1000);

  it("accepts Stripe's own header and refuses everything else", async () => {
    const header = await signStripePayload(body, WHSEC, t);
    expect(header).toMatch(/^t=\d+,v1=[0-9a-f]{64}$/);
    expect(await verifyStripeSignature(body, header, WHSEC, now)).toBe(true);
    expect(await verifyStripeSignature(body, header, "whsec_test_other", now)).toBe(false);
    expect(await verifyStripeSignature(body.replace("paid", "fail"), header, WHSEC, now)).toBe(false);
    expect(await verifyStripeSignature(body, null, WHSEC, now)).toBe(false);
    expect(await verifyStripeSignature(body, "t=abc,v1=zz", WHSEC, now)).toBe(false);
    expect(await verifyStripeSignature(body, `t=${t}`, WHSEC, now)).toBe(false);
  });

  it("refuses a replay older than five minutes", async () => {
    const header = await signStripePayload(body, WHSEC, t - 301);
    expect(await verifyStripeSignature(body, header, WHSEC, now)).toBe(false);
    expect(await verifyStripeSignature(body, await signStripePayload(body, WHSEC, t - 299), WHSEC, now)).toBe(true);
  });

  it("matches any v1 while the secret is being rolled", async () => {
    const good = (await signStripePayload(body, WHSEC, t)).split(",")[1];
    expect(await verifyStripeSignature(body, `t=${t},v1=${"0".repeat(64)},${good}`, WHSEC, now)).toBe(true);
  });

  it("answers 400 to a bad signature and changes nothing", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    const res = await deliver(s.e, s.d, checkoutCompleted(id), "whsec_test_wrong");
    expect(res.status).toBe(400);
    expect((await billingOf(s.e, id))!.status).toBe("pending");
  });
});

describe("deploy is gated only when billing is configured", () => {
  it("with no Stripe config, deploys as before: placed at once, no Checkout, no Stripe call", async () => {
    const s = setup({});
    await invited(s.e, s.d, RIDER);
    const r = await deploy(s.e, s.d, RIDER, { name: "Free", type: "mxbserver", region: "us-east" });
    expect(r.status).toBe(201);
    expect(r.body).not.toHaveProperty("checkoutUrl");
    expect((r.body as { server: { state: string } }).server.state).toBe("provisioning");
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    expect(s.f.stripeCalls()).toHaveLength(0);
    const res = await stripeWebhook(new Request("https://api/v1/stripe/webhook", { method: "POST", body: "{}" }), s.e, s.d);
    expect(res.status).toBe(503);
  });

  it("with only some of the config, stays off", async () => {
    const s = setup({ STRIPE_SECRET_KEY: "sk_test_fixture", STRIPE_WEBHOOK_SECRET: WHSEC });
    await invited(s.e, s.d, RIDER);
    const r = await deploy(s.e, s.d, RIDER, { name: "Free", type: "mxbserver", region: "us-east" });
    expect(r.body).not.toHaveProperty("checkoutUrl");
    expect(s.f.stripeCalls()).toHaveLength(0);
  });

  it("with it, still needs an invite", async () => {
    const s = setup();
    const r = await deploy(s.e, s.d, RIDER, { name: "x", type: "mxbserver", region: "us-east" });
    expect(r.status).toBe(403);
    expect(s.f.stripeCalls()).toHaveLength(0);
  });

  it("with it, holds the server pending and returns Checkout; nothing is ordered until paid", async () => {
    const s = setup();
    const { id, body } = await pendingDeploy(s);
    expect(body.checkoutUrl).toBe("https://checkout.stripe.com/c/pay/cs_test_1");
    expect(body.server.state).toBe("awaiting_payment");
    expect(s.ovh.orderVps).not.toHaveBeenCalled();
    const session = s.f.stripeCalls().find((c) => c.url.endsWith("/checkout/sessions"))!;
    const form = new URLSearchParams(session.body);
    expect(form.get("mode")).toBe("subscription");
    expect(form.get("customer")).toBe("cus_test_rider");
    expect(form.get("line_items[0][price]")).toBe("price_test_mxbserver");
    expect(form.get("line_items[0][quantity]")).toBe("1");
    expect(form.get("subscription_data[metadata][server_id]")).toBe(id);
    expect(form.get("success_url")).toBe(`https://servers.mxbsecure.com/mine?checkout=done&server=${id}`);
    // The pending server counts against the quota.
    expect((await deploy(s.e, s.d, RIDER, { name: "Two", type: "mxbserver", region: "us-east" })).status).toBe(403);
  });

  it("uses the Legacy price for Legacy, and one customer per Steam account", async () => {
    const s = setup();
    await invited(s.e, s.d, RIDER, 2);
    await deploy(s.e, s.d, RIDER, { name: "A", type: "legacy", region: "us-east" });
    await deploy(s.e, s.d, RIDER, { name: "B", type: "mxbserver", region: "us-east" });
    const sessions = s.f.stripeCalls().filter((c) => c.url.endsWith("/checkout/sessions"));
    expect(new URLSearchParams(sessions[0].body).get("line_items[0][price]")).toBe("price_test_legacy");
    expect(s.f.stripeCalls().filter((c) => c.url.endsWith("/customers"))).toHaveLength(1);
  });

  it("refuses before payment when there is no capacity", async () => {
    const s = setup({ ...BILLING, MXB_HOST_SPEND_CAP_USD: "0" });
    await invited(s.e, s.d, RIDER);
    const r = await deploy(s.e, s.d, RIDER, { name: "x", type: "mxbserver", region: "us-east" });
    expect(r).toMatchObject({ status: 409, body: { code: "no_capacity" } });
    expect(s.f.stripeCalls()).toHaveLength(0);
  });
});

describe("the billing state machine", () => {
  it("places the server once checkout.session.completed says paid, once per event", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    const res = await deliver(s.e, s.d, checkoutCompleted(id));
    expect(res.status).toBe(200);
    expect(await billingOf(s.e, id)).toMatchObject({ status: "active", subscription_id: "sub_test_1" });
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    const again = await deliver(s.e, s.d, checkoutCompleted(id));
    expect(await again.json()).toMatchObject({ duplicate: true });
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    const view = (await myHosting(s.e, RIDER, false)).body as { servers: { state: string }[] };
    expect(view.servers[0].state).toBe("provisioning");
  });

  it("also activates on invoice.paid arriving first", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    await deliver(s.e, s.d, invoice("invoice.paid", id, 1));
    expect(await billingOf(s.e, id)).toMatchObject({ status: "active", subscription_id: "sub_test_1" });
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
  });

  it("past due: keeps the server for the grace, then suspends, frees the slot and cancels", async () => {
    const s = setup();
    const { id } = await activeOnSlot(s);
    expect(await usedSlots(s.e)).toBe(1);
    await deliver(s.e, s.d, invoice("invoice.payment_failed", id, 2));
    const pastDue = await billingOf(s.e, id);
    expect(pastDue).toMatchObject({ status: "past_due", grace_until: s.clock.t + GRACE_DAYS * DAY });

    s.clock.t += GRACE_DAYS * DAY - 60_000;
    await billingTick(s.e, s.d);
    expect(await usedSlots(s.e)).toBe(1);

    s.clock.t += 120_000;
    await billingTick(s.e, s.d);
    expect((await billingOf(s.e, id))!.status).toBe("suspended");
    expect(await usedSlots(s.e)).toBe(0);
    expect(s.f.stripeCalls().filter((c) => c.method === "DELETE" && c.url.endsWith("/subscriptions/sub_test_1"))).toHaveLength(1);
    const mine = (await myHosting(s.e, RIDER, false)).body as { servers: unknown[] };
    expect(mine.servers).toHaveLength(0);
  });

  it("a payment inside the grace puts it back to active", async () => {
    const s = setup();
    const { id } = await activeOnSlot(s);
    await deliver(s.e, s.d, invoice("invoice.payment_failed", id, 2));
    await deliver(s.e, s.d, invoice("invoice.paid", id, 3));
    expect(await billingOf(s.e, id)).toMatchObject({ status: "active", grace_until: null });
    s.clock.t += 10 * DAY;
    await billingTick(s.e, s.d);
    expect(await usedSlots(s.e)).toBe(1);
  });

  it("a cancelled subscription suspends after the grace, without cancelling again", async () => {
    const s = setup();
    const { id } = await activeOnSlot(s);
    await deliver(s.e, s.d, {
      id: "evt_test_sub_deleted",
      type: "customer.subscription.deleted",
      data: { object: { id: "sub_test_1", object: "subscription", status: "canceled", metadata: { server_id: id } } },
    });
    expect((await billingOf(s.e, id))!.status).toBe("canceled");
    s.clock.t += GRACE_DAYS * DAY + 1;
    await billingTick(s.e, s.d);
    expect((await billingOf(s.e, id))!.status).toBe("suspended");
    expect(await usedSlots(s.e)).toBe(0);
    expect(s.f.stripeCalls().filter((c) => c.method === "DELETE")).toHaveLength(0);
  });

  it("an expired Checkout drops the pending server", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    await deliver(s.e, s.d, {
      id: "evt_test_expired",
      type: "checkout.session.expired",
      data: { object: { id: "cs_test_1", object: "checkout.session", mode: "subscription", client_reference_id: id, metadata: { server_id: id } } },
    });
    expect((await billingOf(s.e, id))!.status).toBe("expired");
    expect(((await myHosting(s.e, RIDER, false)).body as { servers: unknown[] }).servers).toHaveLength(0);
  });

  it("an unpaid Checkout is dropped by the tick after a day", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    s.clock.t += DAY + 1;
    await billingTick(s.e, s.d);
    expect((await billingOf(s.e, id))!.status).toBe("expired");
    expect(s.f.stripeCalls().some((c) => c.url.endsWith("/checkout/sessions/cs_test_1/expire"))).toBe(true);
  });

  it("an owner deleting a paid server ends it at period end, no refund; idling does not reclaim it", async () => {
    const s = setup({ ...BILLING, MXB_HOST_IDLE_DAYS: "1" });
    const { id } = await activeOnSlot(s);
    s.clock.t += 5 * DAY;
    await hostingTick(s.e, s.d);
    expect(await usedSlots(s.e)).toBe(1);
    expect((await ownerDelete(s.e, s.d, RIDER, id)).status).toBe(200);
    expect((await billingOf(s.e, id))!.status).toBe("ended");
    expect(await usedSlots(s.e)).toBe(0);
    const calls = s.f.stripeCalls();
    const ended = calls.filter((c) => c.method === "POST" && c.url.endsWith("/subscriptions/sub_test_1"));
    expect(ended).toHaveLength(1);
    expect(new URLSearchParams(ended[0].body).get("cancel_at_period_end")).toBe("true");
    expect(calls.filter((c) => c.method === "DELETE")).toHaveLength(0);
    expect(calls.filter((c) => c.url.endsWith("/refunds"))).toHaveLength(0);
  });

  it("an operator delete still cancels the subscription at once", async () => {
    const s = setup();
    const { id } = await activeOnSlot(s);
    expect((await deleteServer(s.e, s.d, id, "operator test")).status).toBe(200);
    expect(s.f.stripeCalls().filter((c) => c.method === "DELETE" && c.url.endsWith("/subscriptions/sub_test_1"))).toHaveLength(1);
  });

  it("an owner's cancellation running out deletes the server, with no grace", async () => {
    const s = setup();
    const { id } = await activeOnSlot(s);
    await deliver(s.e, s.d, {
      id: "evt_test_sub_deleted_owner",
      type: "customer.subscription.deleted",
      data: {
        object: {
          id: "sub_test_1",
          object: "subscription",
          status: "canceled",
          cancellation_details: { reason: "cancellation_requested" },
          metadata: { server_id: id },
        },
      },
    });
    expect((await billingOf(s.e, id))!.status).toBe("ended");
    expect(await usedSlots(s.e)).toBe(0);
    expect(((await myHosting(s.e, RIDER, false)).body as { servers: unknown[] }).servers).toHaveLength(0);
    expect(s.f.stripeCalls().filter((c) => c.method === "DELETE")).toHaveLength(0);
  });

  it("a payment for a server deleted meanwhile cancels the subscription and alerts", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    await ownerDelete(s.e, s.d, RIDER, id);
    await deliver(s.e, s.d, checkoutCompleted(id));
    // Deleted while pending: the late payment is not placed, its subscription is cancelled.
    expect((await billingOf(s.e, id))!.status).toBe("expired");
    expect(s.ovh.orderVps).not.toHaveBeenCalled();
    expect(s.f.stripeCalls().filter((c) => c.method === "DELETE" && c.url.endsWith("/subscriptions/sub_test_1"))).toHaveLength(1);
    const alerts = ((await operatorView(s.e)).body as { alerts: { kind: string; message: string }[] }).alerts;
    expect(alerts.find((a) => a.kind === "billing")?.message).toContain("refund");
  });
});

describe("one payment places one server, once", () => {
  /** Hold every OVH order until released, so concurrent placements overlap the way Workers do. */
  function slowOrders(s: ReturnType<typeof setup>) {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    let next = 100;
    s.ovh.orderVps.mockImplementation(async () => {
      await gate;
      return { orderId: ++next, price: 8.1, currency: "USD" };
    });
    return () => release();
  }

  async function boxCount(e: Env): Promise<number> {
    return (await e.DB.prepare("SELECT COUNT(*) AS n FROM host_boxes").first<{ n: number }>())!.n;
  }

  async function serverOf(e: Env, id: string) {
    return e.DB.prepare("SELECT state, box_id, error FROM host_servers WHERE id = ?")
      .bind(id)
      .first<{ state: string; box_id: string | null; error: string | null }>();
  }

  it("checkout.session.completed and invoice.paid landing at once order one box", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    const release = slowOrders(s);
    const both = Promise.all([deliver(s.e, s.d, checkoutCompleted(id)), deliver(s.e, s.d, invoice("invoice.paid", id, 1))]);
    await new Promise((r) => setTimeout(r, 20));
    release();
    const [a, b] = await both;
    expect([a.status, b.status]).toEqual([200, 200]);
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    expect(await boxCount(s.e)).toBe(1);
    expect(await billingOf(s.e, id)).toMatchObject({ status: "active", subscription_id: "sub_test_1" });
    expect(await serverOf(s.e, id)).toMatchObject({ state: "waiting", error: null });
  });

  it("two concurrent seat attempts for one server: one places, the other stands down", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    const release = slowOrders(s);
    const both = Promise.all([seatPaid(s.e, s.d, id), seatPaid(s.e, s.d, id)]);
    await new Promise((r) => setTimeout(r, 20));
    release();
    expect((await both).sort()).toEqual([ALREADY_SEATED, "ok"].sort());
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    expect(await boxCount(s.e)).toBe(1);
  });

  it("two concurrent places in one pool and region share one box order", async () => {
    const s = setup({});
    await invited(s.e, s.d, RIDER, 2);
    const release = slowOrders(s);
    const both = Promise.all([
      deploy(s.e, s.d, RIDER, { name: "A", type: "mxbserver", region: "us-east" }),
      deploy(s.e, s.d, RIDER, { name: "B", type: "mxbserver", region: "us-east" }),
    ]);
    await new Promise((r) => setTimeout(r, 20));
    release();
    const [a, b] = await both;
    expect([a.status, b.status]).toEqual([201, 201]);
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    expect(await boxCount(s.e)).toBe(1);
    const ids = [a, b].map((r) => (r.body as { server: { id: string } }).server.id);
    const rows = await Promise.all(ids.map((i) => serverOf(s.e, i)));
    expect(rows[0]!.box_id).toBeTruthy();
    expect(rows[1]!.box_id).toBe(rows[0]!.box_id);
  });

  it("a failed order refunds, cancels, and leaves the server failed with one short line", async () => {
    const s = setup();
    const { id } = await pendingDeploy(s);
    s.ovh.orderVps.mockRejectedValueOnce(new Error("OVH said no (test)"));
    expect((await deliver(s.e, s.d, checkoutCompleted(id))).status).toBe(200);
    const server = await serverOf(s.e, id);
    expect(server).toMatchObject({ state: "failed", error: REFUNDED_ERROR });
    expect(server!.error).not.toContain("OVH");
    const calls = s.f.stripeCalls();
    const refundAt = calls.findIndex((c) => c.url.endsWith("/refunds"));
    expect(new URLSearchParams(calls[refundAt].body).get("payment_intent")).toBe("pi_test_1");
    const cancelAt = calls.findIndex((c) => c.method === "DELETE" && c.url.endsWith("/subscriptions/sub_test_1"));
    expect(cancelAt).toBeGreaterThan(refundAt);
    const billed = await s.e.DB.prepare("SELECT status, refund_id, refunded_at FROM host_billing WHERE server_id = ?")
      .bind(id)
      .first<{ status: string; refund_id: string | null; refunded_at: number | null }>();
    expect(billed).toMatchObject({ status: "ended", refund_id: "re_test_1", refunded_at: s.clock.t });
    const alerts = ((await operatorView(s.e)).body as { alerts: { kind: string; message: string }[] }).alerts;
    expect(alerts.some((a) => a.kind === "billing")).toBe(false);
    expect(alerts.find((a) => a.kind === "order_failed")?.message).toContain("OVH said no");
    // The other event for the same payment changes nothing.
    await deliver(s.e, s.d, invoice("invoice.paid", id, 1));
    expect(s.ovh.orderVps).toHaveBeenCalledTimes(1);
    expect(s.f.stripeCalls().filter((c) => c.url.endsWith("/refunds"))).toHaveLength(1);
  });

  it("a refund that fails is alerted with the cause, and the owner is told it is on its way", async () => {
    const s = setup(BILLING, { refundFails: true });
    const { id } = await pendingDeploy(s);
    s.ovh.orderVps.mockRejectedValueOnce(new Error("OVH said no (test)"));
    await deliver(s.e, s.d, checkoutCompleted(id));
    expect(await serverOf(s.e, id)).toMatchObject({ state: "failed", error: REFUND_PENDING_ERROR });
    expect(s.f.stripeCalls().some((c) => c.method === "DELETE" && c.url.endsWith("/subscriptions/sub_test_1"))).toBe(true);
    const alerts = ((await operatorView(s.e)).body as { alerts: { kind: string; message: string }[] }).alerts;
    const billing = alerts.find((a) => a.kind === "billing")!.message;
    expect(billing).toContain("the order failed");
    expect(billing).toContain("test refund refused");
  });
});

describe("site and operator routes", () => {
  async function call(e: Env, d: Deps, method: string, path: string, steamId = RIDER) {
    const { sealToken, SESSION_COOKIE } = await import("../src/websession");
    const cookie = `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId, name: "R", exp: Date.now() + 60_000 }, "session-secret")}`;
    const req = new Request(`https://api.mxbsecure.com${path}`, {
      method,
      headers: { Cookie: cookie, Origin: "https://servers.mxbsecure.com", ...(method === "GET" ? {} : { "Content-Type": "application/json" }) },
      body: method === "GET" ? undefined : "{}",
    });
    const res = await hostingWebRoutes(req, new URL(req.url), e, "https://servers.mxbsecure.com", d);
    return { status: res.status, body: (await res.json()) as Record<string, unknown> };
  }

  it("shows prices, status and a portal link to the owner, and MRR to operators", async () => {
    const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
    const { id } = await pendingDeploy(s);
    let mine = await call(s.e, s.d, "GET", "/v1/web/hosting/billing");
    expect(mine.status).toBe(200);
    expect(mine.body).toMatchObject({
      enabled: true,
      portal: true,
      prices: [
        { type: "mxbserver", cents: 500, currency: "usd" },
        { type: "legacy", cents: 800, currency: "usd" },
      ],
      servers: [{ serverId: id, status: "pending", amountCents: 500 }],
    });
    const resumed = await call(s.e, s.d, "POST", `/v1/web/hosting/billing/servers/${id}/checkout`);
    expect(resumed.body).toEqual({ url: "https://checkout.stripe.com/c/pay/cs_test_2" });

    await deliver(s.e, s.d, checkoutCompleted(id, 2));
    mine = await call(s.e, s.d, "GET", "/v1/web/hosting/billing");
    expect((mine.body.servers as { status: string }[])[0].status).toBe("active");
    const portal = await call(s.e, s.d, "POST", "/v1/web/hosting/billing/portal");
    expect(portal.body).toEqual({ url: "https://billing.stripe.com/p/session/test_portal" });

    const op = await call(s.e, s.d, "GET", "/v1/web/admin/hosting", BOSS);
    expect(op.body.billing).toMatchObject({ enabled: true, currency: "usd", mrrCents: 500, active: 1 });
    expect((op.body.servers as { id: string; billingStatus: string; mrrCents: number }[])[0]).toMatchObject({
      id,
      billingStatus: "active",
      mrrCents: 500,
    });
  });

  it("shows the next bill date and invoices; cancel shows the end date, resume takes it back", async () => {
    const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
    const { id } = await activeOnSlot(s);
    let mine = await call(s.e, s.d, "GET", "/v1/web/hosting/billing");
    expect((mine.body.servers as unknown[])[0]).toMatchObject({
      serverId: id,
      status: "active",
      amountCents: 500,
      nextBillAt: PERIOD_END_S * 1000,
      cancelAtPeriodEnd: false,
      endsAt: null,
    });
    // Drafts are left out; a PDF link that isn't Stripe's is dropped.
    expect(mine.body.invoices).toEqual([
      { id: "in_test_2", number: "TEST-0002", date: (PERIOD_END_S - 30 * 86400) * 1000, amountCents: 500, currency: "usd", status: "paid", pdf: "https://pay.stripe.com/invoice/acct_test/in_test_2/pdf" },
      { id: "in_test_4", number: null, date: (PERIOD_END_S - 60 * 86400) * 1000, amountCents: 500, currency: "usd", status: "open", pdf: null },
    ]);

    const cancelled = await call(s.e, s.d, "POST", `/v1/web/hosting/billing/servers/${id}/cancel`);
    expect(cancelled).toEqual({ status: 200, body: { serverId: id, nextBillAt: null, cancelAtPeriodEnd: true, endsAt: PERIOD_END_S * 1000 } });
    const sent = s.f.stripeCalls().filter((c) => c.method === "POST" && c.url.endsWith("/subscriptions/sub_test_1"));
    expect(new URLSearchParams(sent.at(-1)!.body).get("cancel_at_period_end")).toBe("true");
    expect(s.f.stripeCalls().filter((c) => c.method === "DELETE" || c.url.endsWith("/refunds"))).toHaveLength(0);
    mine = await call(s.e, s.d, "GET", "/v1/web/hosting/billing");
    expect((mine.body.servers as unknown[])[0]).toMatchObject({ status: "active", cancelAtPeriodEnd: true, endsAt: PERIOD_END_S * 1000 });
    // Still running until then.
    expect(await usedSlots(s.e)).toBe(1);

    const resumed = await call(s.e, s.d, "POST", `/v1/web/hosting/billing/servers/${id}/resume`);
    expect(resumed.body).toEqual({ serverId: id, nextBillAt: PERIOD_END_S * 1000, cancelAtPeriodEnd: false, endsAt: null });
    expect(new URLSearchParams(s.f.stripeCalls().at(-1)!.body).get("cancel_at_period_end")).toBe("false");
  });

  it("cancel and resume need an active plan", async () => {
    const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
    const { id } = await pendingDeploy(s);
    expect((await call(s.e, s.d, "POST", `/v1/web/hosting/billing/servers/${id}/cancel`)).status).toBe(409);
    expect((await call(s.e, s.d, "POST", `/v1/web/hosting/billing/servers/${id}/resume`)).status).toBe(409);
  });

  it("another account gets 404 for someone else's server, and nothing changes", async () => {
    const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
    const { id } = await activeOnSlot(s);
    const before = s.f.stripeCalls().length;
    for (const [method, path] of [
      ["POST", `/v1/web/hosting/billing/servers/${id}/cancel`],
      ["POST", `/v1/web/hosting/billing/servers/${id}/resume`],
      ["POST", `/v1/web/hosting/billing/servers/${id}/checkout`],
      ["DELETE", `/v1/web/hosting/servers/${id}`],
    ] as const) {
      expect((await call(s.e, s.d, method, path, OTHER)).status).toBe(404);
    }
    expect(s.f.stripeCalls().length).toBe(before);
    expect((await billingOf(s.e, id))!.status).toBe("active");
    expect(await usedSlots(s.e)).toBe(1);
    // Nor does their billing list it.
    const theirs = await call(s.e, s.d, "GET", "/v1/web/hosting/billing", OTHER);
    expect(theirs.body).toMatchObject({ servers: [], invoices: [], portal: false });
  });

  describe("DELETE /v1/web/hosting/servers/:id, in each state", () => {
    const del = (s: ReturnType<typeof setup>, id: string) => call(s.e, s.d, "DELETE", `/v1/web/hosting/servers/${id}`);
    const gone = async (s: ReturnType<typeof setup>) => ((await myHosting(s.e, RIDER, false)).body as { servers: unknown[] }).servers;

    it("pending: drops the Checkout and the server", async () => {
      const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
      const { id } = await pendingDeploy(s);
      expect((await del(s, id)).status).toBe(200);
      expect(await gone(s)).toHaveLength(0);
      expect((await billingOf(s.e, id))!.status).toBe("expired");
      expect(s.f.stripeCalls().some((c) => c.url.endsWith("/checkout/sessions/cs_test_1/expire"))).toBe(true);
      expect(s.f.stripeCalls().filter((c) => c.url.includes("/subscriptions") || c.url.endsWith("/refunds"))).toHaveLength(0);
    });

    it("failed and refunded: just removes it, no second refund or cancel", async () => {
      const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
      const { id } = await pendingDeploy(s);
      s.ovh.orderVps.mockRejectedValueOnce(new Error("OVH said no (test)"));
      await deliver(s.e, s.d, checkoutCompleted(id));
      const before = s.f.stripeCalls().length;
      expect((await del(s, id)).status).toBe(200);
      expect(await gone(s)).toHaveLength(0);
      expect(s.f.stripeCalls().length).toBe(before);
      expect((await billingOf(s.e, id))!.status).toBe("ended");
    });

    it("active: frees the slot and stops renewal at period end", async () => {
      const s = setup({ ...BILLING, MXB_WEB_SESSION_KEY: "session-secret" });
      const { id } = await activeOnSlot(s);
      expect((await del(s, id)).status).toBe(200);
      expect(await gone(s)).toHaveLength(0);
      expect(await usedSlots(s.e)).toBe(0);
      const last = s.f.stripeCalls().at(-1)!;
      expect(last).toMatchObject({ method: "POST", url: "https://api.stripe.com/v1/subscriptions/sub_test_1" });
      expect(new URLSearchParams(last.body).get("cancel_at_period_end")).toBe("true");
      // Deleting it again is a 404, not a second cancel.
      expect((await del(s, id)).status).toBe(404);
    });

    it("free (billing off): just removes it", async () => {
      const s = setup({ MXB_WEB_SESSION_KEY: "session-secret" });
      await invited(s.e, s.d, RIDER);
      const r = await deploy(s.e, s.d, RIDER, { name: "Free", type: "mxbserver", region: "us-east" });
      const id = (r.body as { server: { id: string } }).server.id;
      expect((await del(s, id)).status).toBe(200);
      expect(await gone(s)).toHaveLength(0);
      expect(s.f.stripeCalls()).toHaveLength(0);
    });
  });

  it("says billing is off when it is", async () => {
    const s = setup({ MXB_WEB_SESSION_KEY: "session-secret" });
    const mine = await call(s.e, s.d, "GET", "/v1/web/hosting/billing");
    expect(mine.body).toMatchObject({ enabled: false, prices: [], portal: false, servers: [] });
    expect((await call(s.e, s.d, "POST", "/v1/web/hosting/billing/portal")).status).toBe(404);
  });
});
