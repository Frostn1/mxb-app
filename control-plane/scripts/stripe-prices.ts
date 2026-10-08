/**
 * Make the two paid-hosting prices on Creste LLC's Stripe account. Run once per mode (test, then
 * live), locally:
 *
 *   STRIPE_SECRET_KEY=sk_test_... bun scripts/stripe-prices.ts
 *
 * Makes one product, "MX Bikes server hosting", with two monthly USD prices: $5 (mxbserver) and
 * $8 (Legacy), looked up by `lookup_key` so a second run reuses them instead of making more. It
 * prints the two price ids; put them in wrangler.jsonc as `STRIPE_PRICE_MXBSERVER` and
 * `STRIPE_PRICE_LEGACY`. The amounts match `PRICE_CENTS` in src/billing.ts.
 */

const key = process.env.STRIPE_SECRET_KEY;
if (!key) {
  console.error("Set STRIPE_SECRET_KEY first.");
  process.exit(1);
}

const PRICES = [
  { var: "STRIPE_PRICE_MXBSERVER", lookup: "mxb_hosting_mxbserver_monthly", cents: 500, nickname: "mxbserver" },
  { var: "STRIPE_PRICE_LEGACY", lookup: "mxb_hosting_legacy_monthly", cents: 800, nickname: "Legacy" },
];

async function call<T>(method: string, path: string, params: Record<string, string> = {}): Promise<T> {
  const body = new URLSearchParams(params).toString();
  const res = await fetch(`https://api.stripe.com/v1${path}${method === "GET" && body ? `?${body}` : ""}`, {
    method,
    headers: { Authorization: `Bearer ${key}`, "Content-Type": "application/x-www-form-urlencoded" },
    body: method === "GET" ? undefined : body,
  });
  const json = (await res.json()) as T & { error?: { message: string } };
  if (!res.ok) throw new Error(json.error?.message ?? `Stripe ${res.status}`);
  return json;
}

const existing = await call<{ data: { id: string; lookup_key: string; product: string }[] }>("GET", "/prices", {
  "lookup_keys[0]": PRICES[0].lookup,
  "lookup_keys[1]": PRICES[1].lookup,
});
let product = existing.data[0]?.product;
if (!product) {
  product = (await call<{ id: string }>("POST", "/products", { name: "MX Bikes server hosting" })).id;
}
for (const p of PRICES) {
  let id = existing.data.find((e) => e.lookup_key === p.lookup)?.id;
  if (!id) {
    id = (
      await call<{ id: string }>("POST", "/prices", {
        product,
        currency: "usd",
        unit_amount: String(p.cents),
        "recurring[interval]": "month",
        lookup_key: p.lookup,
        nickname: p.nickname,
      })
    ).id;
  }
  console.log(`"${p.var}": "${id}",`);
}

export {};
