import { describe, expect, it } from "vitest";
import { OvhClient, OvhError, ovhCredentials, ovhSignature } from "../src/ovh";

/**
 * The OVH client against recorded-shape responses. No credentials and no network: every call
 * goes to `mock`, which answers from a route table shaped like OVH's documented responses.
 */

const creds = {
  endpoint: "https://api.us.ovhcloud.com/1.0",
  applicationKey: "app-key",
  applicationSecret: "app-secret",
  consumerKey: "consumer-key",
};

type Call = { method: string; url: string; body: unknown; headers: Record<string, string> };

function mock(routes: Record<string, unknown>): { fetch: typeof fetch; calls: Call[] } {
  const calls: Call[] = [];
  const f = async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const body = init?.body ? JSON.parse(String(init.body)) : undefined;
    calls.push({ method, url, body, headers: (init?.headers ?? {}) as Record<string, string> });
    if (url.endsWith("/auth/time")) return new Response("1791331200");
    const key = `${method} ${url.replace(creds.endpoint, "")}`;
    if (!(key in routes)) return new Response(JSON.stringify({ message: `no route ${key}` }), { status: 404 });
    const value = routes[key];
    if (value instanceof Response) return value;
    return new Response(value === null ? "null" : JSON.stringify(value), { status: 200 });
  };
  return { fetch: f as unknown as typeof fetch, calls };
}

describe("OVH signing", () => {
  it("matches OVH's documented $1$ + sha1(AS+CK+METHOD+URL+BODY+TS)", async () => {
    const url = "https://api.us.ovhcloud.com/1.0/order/cart";
    const body = '{"ovhSubsidiary":"US"}';
    // Computed outside this code: sha1("app-secret+consumer-key+POST+<url>+<body>+1791331200").
    const expected = "$1$f107694ad0c3089f21d93dad15266e97d4e57695";
    expect(await ovhSignature("app-secret", "consumer-key", "POST", url, body, 1791331200)).toBe(expected);
  });

  it("signs with OVH's clock and sends the four headers", async () => {
    const m = mock({ "GET /vps/vps-1.vps.ovh.us": { state: "running" } });
    // Our clock is an hour behind OVH's; the timestamp must follow theirs.
    const client = new OvhClient(creds, m.fetch, () => (1791331200 - 3600) * 1000);
    await client.vps("vps-1.vps.ovh.us");
    const call = m.calls.find((c) => c.url.endsWith("/vps/vps-1.vps.ovh.us"))!;
    expect(call.headers["X-Ovh-Application"]).toBe("app-key");
    expect(call.headers["X-Ovh-Consumer"]).toBe("consumer-key");
    expect(call.headers["X-Ovh-Timestamp"]).toBe("1791331200");
    expect(call.headers["X-Ovh-Signature"]).toBe(
      await ovhSignature("app-secret", "consumer-key", "GET", call.url, "", 1791331200),
    );
  });

  it("is off without all three secrets", () => {
    expect(ovhCredentials({} as Env)).toBeNull();
    expect(ovhCredentials({ OVH_APPLICATION_KEY: "a", OVH_APPLICATION_SECRET: "b" } as unknown as Env)).toBeNull();
    expect(
      ovhCredentials({ OVH_APPLICATION_KEY: "a", OVH_APPLICATION_SECRET: "b", OVH_CONSUMER_KEY: "c" } as unknown as Env)?.endpoint,
    ).toBe("https://api.us.ovhcloud.com/1.0");
  });
});

describe("ordering a VPS", () => {
  const routes = {
    "POST /order/cart": { cartId: "cart-1", expire: "2026-10-08T00:00:00Z", items: [] },
    "POST /order/cart/cart-1/assign": null,
    "POST /order/cart/cart-1/vps": { itemId: 42, cartId: "cart-1", productId: "vps" },
    "POST /order/cart/cart-1/item/42/configuration": { id: 1, label: "vps_datacenter", value: "SYD" },
    "POST /order/cart/cart-1/vps/options": { itemId: 43 },
    "GET /order/cart/cart-1/checkout": { prices: { withTax: { value: 8.1, currencyCode: "USD" } }, orderId: null },
    "POST /order/cart/cart-1/checkout": { orderId: 987654, prices: { withTax: { value: 8.1, currencyCode: "USD" } } },
  };

  it("walks the cart: plan, datacenter, OS, the mandatory add-ons, checkout with the default method", async () => {
    const m = mock(routes);
    const client = new OvhClient(creds, m.fetch);
    const order = await client.orderVps({
      subsidiary: "US",
      planCode: "vps-2027-model1-ca",
      datacenter: "SYD",
      os: "Ubuntu 24.04",
      addons: ["option-linux-ca", "option-storage-local-2027-model1-ca", "option-auto-backup-2027-1-model1-ca"],
    });
    expect(order).toEqual({ orderId: 987654, price: 8.1, currency: "USD" });
    const ovh = m.calls.filter((c) => !c.url.endsWith("/auth/time")).map((c) => [c.method, c.url.replace(creds.endpoint, ""), c.body]);
    expect(ovh).toEqual([
      ["POST", "/order/cart", { ovhSubsidiary: "US" }],
      ["POST", "/order/cart/cart-1/assign", undefined],
      ["POST", "/order/cart/cart-1/vps", { planCode: "vps-2027-model1-ca", duration: "P1M", pricingMode: "default", quantity: 1 }],
      ["POST", "/order/cart/cart-1/item/42/configuration", { label: "vps_datacenter", value: "SYD" }],
      ["POST", "/order/cart/cart-1/item/42/configuration", { label: "vps_os", value: "Ubuntu 24.04" }],
      ["POST", "/order/cart/cart-1/vps/options", { itemId: 42, planCode: "option-linux-ca", duration: "P1M", pricingMode: "default", quantity: 1 }],
      ["POST", "/order/cart/cart-1/vps/options", { itemId: 42, planCode: "option-storage-local-2027-model1-ca", duration: "P1M", pricingMode: "default", quantity: 1 }],
      ["POST", "/order/cart/cart-1/vps/options", { itemId: 42, planCode: "option-auto-backup-2027-1-model1-ca", duration: "P1M", pricingMode: "default", quantity: 1 }],
      ["GET", "/order/cart/cart-1/checkout", undefined],
      ["POST", "/order/cart/cart-1/checkout", { autoPayWithPreferredPaymentMethod: true, waiveRetractationPeriod: false }],
    ]);
  });

  it("surfaces OVH's message on a refusal", async () => {
    const m = mock({
      ...routes,
      "POST /order/cart/cart-1/checkout": new Response(JSON.stringify({ message: "No payment method" }), { status: 400 }),
    });
    const client = new OvhClient(creds, m.fetch);
    await expect(
      client.orderVps({ subsidiary: "US", planCode: "vps-2027-model1", datacenter: "US-EAST-VA", os: "Ubuntu 24.04", addons: [] }),
    ).rejects.toThrow(OvhError);
  });
});

describe("a delivered VPS", () => {
  const routes = {
    "GET /me/order/987654/details": [11, 12],
    "GET /me/order/987654/details/11": { domain: "*", description: "Option Linux" },
    "GET /me/order/987654/details/12": { domain: "vps-abc123.vps.ovh.us", description: "VPS-1 2026" },
    "GET /vps/vps-abc123.vps.ovh.us/ips": ["2604:2dc0:101:200::1", "51.81.10.20"],
    "GET /vps/vps-abc123.vps.ovh.us/serviceInfos": { expiration: "2026-11-07", status: "ok", renew: { automatic: true } },
    "GET /vps/vps-abc123.vps.ovh.us/images/available": ["img-1", "img-2"],
    "GET /vps/vps-abc123.vps.ovh.us/images/available/img-1": { id: "img-1", name: "Ubuntu 24.04" },
    "GET /vps/vps-abc123.vps.ovh.us/images/available/img-2": { id: "img-2", name: "Debian 12" },
    "POST /vps/vps-abc123.vps.ovh.us/rebuild": { id: 5, state: "todo", type: "rebuild" },
    "GET /vps/vps-abc123.vps.ovh.us/tasks?state=todo": [],
    "GET /vps/vps-abc123.vps.ovh.us/tasks?state=doing": [5],
  };

  it("finds the service name, its IPv4 and its renewal date", async () => {
    const client = new OvhClient(creds, mock(routes).fetch);
    expect(await client.deliveredService(987654)).toBe("vps-abc123.vps.ovh.us");
    expect(await client.ipv4("vps-abc123.vps.ovh.us")).toBe("51.81.10.20");
    expect(await client.renewsAt("vps-abc123.vps.ovh.us")).toBe(Date.parse("2026-11-07"));
  });

  it("is not delivered while the detail has no VPS name", async () => {
    const client = new OvhClient(creds, mock({ ...routes, "GET /me/order/987654/details": [11] }).fetch);
    expect(await client.deliveredService(987654)).toBeNull();
  });

  it("rebuilds onto the named image with our key and no emailed password", async () => {
    const m = mock(routes);
    const client = new OvhClient(creds, m.fetch);
    await client.rebuild("vps-abc123.vps.ovh.us", "Debian 12", "ssh-ed25519 AAAA test");
    const rebuild = m.calls.find((c) => c.method === "POST" && c.url.endsWith("/rebuild"))!;
    expect(rebuild.body).toEqual({ imageId: "img-2", publicSshKey: "ssh-ed25519 AAAA test", doNotSendPassword: true });
    expect(await client.busy("vps-abc123.vps.ovh.us")).toBe(true);
  });
});
