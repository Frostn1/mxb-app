import { describe, expect, it, vi } from "vitest";
import { signStripePayload, stripeWebhook } from "../src/billing";
import { SUPPORTER_PACK_SLUG, parseCatalog } from "../src/blendshop";
import { defaultDeps } from "../src/hosting";
import { isSupporter } from "../src/supporters";
import { packPrice, startSupporterCheckout } from "../src/supporterpack";
import { d1 } from "./d1sqlite";

// Test-mode style values only. No real Steam ids, keys or Stripe objects.
const WHSEC = "whsec_test_0123456789abcdef";
const RIDER = "76561190000000002";
const ACCOUNT = { id: "acc-1", steam_id: RIDER };

function fakeStripe() {
  const calls: { url: string; body: URLSearchParams }[] = [];
  const f = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, body: new URLSearchParams(typeof init?.body === "string" ? init.body : "") });
    const json = (b: unknown, status = 200) => new Response(JSON.stringify(b), { status });
    if (url === "https://api.stripe.com/v1/customers") return json({ id: "cus_test_rider" });
    if (url === "https://api.stripe.com/v1/checkout/sessions") {
      return json({ id: "cs_test_pack", url: "https://checkout.stripe.com/c/pay/cs_test_pack" });
    }
    return json({ error: { message: "unexpected" } }, 400);
  });
  return { fetch: f as unknown as typeof fetch, calls };
}

function deployment(vars: Record<string, string> = {}) {
  const e = {
    DB: d1(),
    MXB_WEB_SESSION_KEY: "session-secret",
    MXB_SITE_ORIGIN: "https://mxbsecure.com",
    MXB_BLENDS_SHOP: "test",
    STRIPE_SECRET_KEY: "sk_test_fixture",
    STRIPE_WEBHOOK_SECRET: WHSEC,
    MXB_TERMS_VERSION: "2026-10-12",
    ...vars,
  } as unknown as Env;
  return { e, s: fakeStripe() };
}

async function deliver(d: ReturnType<typeof deployment>, event: unknown) {
  const raw = JSON.stringify(event);
  const sig = await signStripePayload(raw, WHSEC, Math.floor(Date.now() / 1000));
  const req = new Request("https://api.mxbsecure.com/v1/stripe/webhook", {
    method: "POST",
    headers: { "Stripe-Signature": sig },
    body: raw,
  });
  return stripeWebhook(req, d.e, defaultDeps(d.e, d.s.fetch));
}

async function checkout(d: ReturnType<typeof deployment>, body: Record<string, unknown> = { agree: true, waiver: true }) {
  return startSupporterCheckout(d.e, defaultDeps(d.e, d.s.fetch), ACCOUNT, body);
}

async function payFor(d: ReturnType<typeof deployment>) {
  expect((await checkout(d)).status).toBe(200);
  const order = await d.e.DB.prepare("SELECT id, checkout_session_id FROM blend_orders").first<{
    id: string;
    checkout_session_id: string;
  }>();
  await deliver(d, {
    id: "evt_test_pack_paid",
    type: "checkout.session.completed",
    data: {
      object: {
        id: order!.checkout_session_id,
        mode: "payment",
        payment_status: "paid",
        payment_intent: "pi_test_pack",
        amount_subtotal: 1000,
        amount_total: 1000,
        currency: "usd",
        total_details: { amount_tax: 0 },
        metadata: { kind: "blend", order_id: order!.id, slug: SUPPORTER_PACK_SLUG },
      },
    },
  });
  return order!;
}

describe("the supporter pack", () => {
  it("is off until the shop and the Terms version are set", async () => {
    expect((await checkout(deployment({ MXB_BLENDS_SHOP: "off" }))).status).toBe(503);
    expect((await checkout(deployment({ MXB_TERMS_VERSION: "" }))).status).toBe(503);
  });

  it("needs the Terms and the immediate-access waiver", async () => {
    const d = deployment();
    expect((await checkout(d, { agree: true })).status).toBe(400);
    expect((await checkout(d, { waiver: true })).status).toBe(400);
  });

  it("opens Checkout at the pack's price and grants nothing until paid", async () => {
    const d = deployment({ MXB_SUPPORTER_PACK_CENTS: "1200", MXB_SUPPORTER_PACK_CURRENCY: "EUR" });
    const r = await checkout(d);
    expect(r).toMatchObject({ status: 200, body: { url: "https://checkout.stripe.com/c/pay/cs_test_pack" } });
    const sent = d.s.calls.find((c) => c.url.endsWith("/checkout/sessions"))!.body;
    expect(sent.get("line_items[0][price_data][unit_amount]")).toBe("1200");
    expect(sent.get("line_items[0][price_data][currency]")).toBe("eur");
    expect(sent.get("metadata[slug]")).toBe(SUPPORTER_PACK_SLUG);
    expect(await isSupporter(d.e, RIDER)).toBe(false);
  });

  it("paid: the pack, and no Blend", async () => {
    const d = deployment();
    await payFor(d);
    expect(await isSupporter(d.e, RIDER)).toBe(true);
    const blends = await d.e.DB.prepare("SELECT COUNT(*) AS n FROM blend_entitlements").first<{ n: number }>();
    expect(blends!.n).toBe(0);
    expect((await checkout(d)).status).toBe(409);
  });

  it("a full refund takes it back; a chargeback too, until it is won", async () => {
    const d = deployment();
    await payFor(d);
    await deliver(d, {
      id: "evt_test_pack_dp",
      type: "charge.dispute.created",
      data: { object: { payment_intent: "pi_test_pack", status: "needs_response" } },
    });
    expect(await isSupporter(d.e, RIDER)).toBe(false);
    await deliver(d, {
      id: "evt_test_pack_dw",
      type: "charge.dispute.closed",
      data: { object: { payment_intent: "pi_test_pack", status: "won" } },
    });
    expect(await isSupporter(d.e, RIDER)).toBe(true);
    await deliver(d, {
      id: "evt_test_pack_rf",
      type: "charge.refunded",
      data: { object: { payment_intent: "pi_test_pack", amount: 1000, amount_refunded: 1000, refunded: true } },
    });
    expect(await isSupporter(d.e, RIDER)).toBe(false);
  });

  it("the price falls back to $10 when the setting is odd", () => {
    expect(packPrice({} as Env)).toEqual({ cents: 1000, currency: "usd" });
    expect(packPrice({ MXB_SUPPORTER_PACK_CENTS: "abc", MXB_SUPPORTER_PACK_CURRENCY: "euro" } as unknown as Env)).toEqual({
      cents: 1000,
      currency: "usd",
    });
  });

  it("can't be listed as a Blend on the site", () => {
    const items = parseCatalog({
      format: "mxb-blends/1",
      items: [{ slug: SUPPORTER_PACK_SLUG, name: "Sneaky", priceCents: 100 }],
    });
    expect(items?.size).toBe(0);
  });
});
