/**
 * The supporter pack, bought once with Stripe.
 *
 * Sold through the Blends shop's own machinery (`blendshop.ts`): the same `blend_orders` table,
 * the same Checkout settings, the same webhook, receipts in the same billing portal, and the
 * same refund and chargeback handling. The pack is one reserved product, `SUPPORTER_PACK_SLUG`,
 * and what a paid order grants is a row in `supporters` (`supporters.ts`) rather than a Blend.
 * A full refund or a chargeback revokes that row; a won dispute restores it.
 *
 * Bought from the app (`POST /v1/supporter/checkout`, signed in), so the price is set here, not
 * in the site's Blends list: `MXB_SUPPORTER_PACK_CENTS` (default 1000) in
 * `MXB_SUPPORTER_PACK_CURRENCY` (default usd, the shop's currency). The Terms version the buyer
 * agrees to is the deployment's `MXB_TERMS_VERSION`; without it, or with the shop shut, the
 * route answers 503 and the app offers Buy Me a Coffee instead.
 */

import { BANNED, isBanned } from "./bans";
import { customerFor, stripe } from "./billing";
import { newRef, shopConfig, SUPPORTER_PACK_SLUG } from "./blendshop";
import { type Deps } from "./hosting";
import { steamIdFor } from "./steamlink";
import { isSupporter } from "./supporters";

const CHECKOUT_TTL_S = 60 * 60;
const TERMS_RE = /^\d{4}-\d{2}-\d{2}$/;

export interface PackPrice {
  cents: number;
  currency: string;
}

/** The pack's price for this deployment. Anything odd falls back to the default. */
export function packPrice(env: Env): PackPrice {
  const cents = Number(env.MXB_SUPPORTER_PACK_CENTS ?? "");
  const currency = (env.MXB_SUPPORTER_PACK_CURRENCY ?? "").trim().toLowerCase();
  return {
    cents: Number.isInteger(cents) && cents >= 100 && cents <= 100_000 ? cents : 1000,
    currency: /^[a-z]{3}$/.test(currency) ? currency : "usd",
  };
}

type Result = { status: number; body: Record<string, unknown> };

/**
 * `POST /v1/supporter/checkout` `{ agree: true, waiver: true }`: a Checkout URL for the pack.
 * `waiver` is the buyer asking for it now and accepting that all sales are final, as with Blends.
 */
export async function startSupporterCheckout(
  env: Env,
  deps: Deps,
  account: { id: string; steam_id: string | null },
  body: Record<string, unknown>,
): Promise<Result> {
  const cfg = shopConfig(env);
  const terms = env.MXB_TERMS_VERSION?.trim();
  if (!cfg || !terms || !TERMS_RE.test(terms)) {
    return { status: 503, body: { error: "The supporter pack isn't on sale yet." } };
  }
  const steamId = await steamIdFor(env, account);
  if (!steamId) return { status: 409, body: { error: "Sign in with Steam first.", code: "steam" } };
  if (body.agree !== true) return { status: 400, body: { error: "Agree to the Terms first." } };
  if (body.waiver !== true) return { status: 400, body: { error: "Confirm immediate access first." } };
  if (await isBanned(env, { steamId })) return { status: 403, body: { error: BANNED } };
  if (await isSupporter(env, steamId)) {
    return { status: 409, body: { error: "You have the pack already.", code: "owned" } };
  }

  const customerId = await customerFor(env, deps, cfg, steamId);
  if (!customerId) return { status: 502, body: { error: "Payment couldn't be started. Try again." } };

  const price = packPrice(env);
  const id = crypto.randomUUID();
  const ref = newRef();
  const now = deps.now();
  // `kind: "blend"` on purpose: it is what routes the webhook through the shop's handling.
  const meta = { kind: "blend", order_id: id, order_ref: ref, slug: SUPPORTER_PACK_SLUG, steam_id: steamId };
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
            currency: price.currency,
            unit_amount: price.cents,
            tax_behavior: "exclusive",
            product_data: {
              name: "Supporter pack",
              tax_code: cfg.taxCode,
              metadata: { slug: SUPPORTER_PACK_SLUG },
            },
          },
        },
      },
      automatic_tax: { enabled: true },
      customer_update: { address: "auto", name: "auto" },
      billing_address_collection: "required",
      invoice_creation: { enabled: true, invoice_data: { description: `Order ${ref}`, metadata: meta } },
      payment_intent_data: { description: `Supporter pack, order ${ref}`, metadata: meta },
      metadata: meta,
      expires_at: Math.floor(now / 1000) + CHECKOUT_TTL_S,
      success_url: `${cfg.site}/account?checkout=done&order=${ref}`,
      cancel_url: `${cfg.site}/account?checkout=canceled`,
    },
    `mxb-supporter-checkout-${id}`,
  );
  if (!session.ok) {
    console.error(JSON.stringify({ msg: "supporter checkout", status: session.status, error: session.error }));
    return { status: 502, body: { error: "Payment couldn't be started. Try again." } };
  }
  await env.DB.prepare(
    `INSERT INTO blend_orders (id, ref, steam_id, slug, name, status, price_cents, currency, customer_id, checkout_session_id,
                               terms_version, agreed_at, waiver_at, created_at, updated_at)
     VALUES (?, ?, ?, ?, 'Supporter pack', 'pending', ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  )
    .bind(id, ref, steamId, SUPPORTER_PACK_SLUG, price.cents, price.currency, customerId, session.body.id, terms, now, now, now, now)
    .run();
  console.log(JSON.stringify({ msg: "supporter checkout", order: ref }));
  return { status: 200, body: { url: session.body.url, ref } };
}
