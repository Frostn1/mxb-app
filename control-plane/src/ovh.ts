/**
 * A small OVHcloud API client: exactly the calls user server deploy makes, nothing else.
 *
 * Credentials are three Worker secrets Sean creates (`OVH_APPLICATION_KEY`,
 * `OVH_APPLICATION_SECRET`, `OVH_CONSUMER_KEY`). The consumer key should be limited to:
 *
 *   GET/POST /order/cart, GET/POST /order/cart/*, GET /me/order/*, GET /vps, GET /vps/*,
 *   POST /vps/*\/rebuild
 *
 * There is deliberately no termination call: boxes are cancelled by hand at OVH.
 *
 * Signing follows OVH's documented scheme: `X-Ovh-Signature` is `$1$` + the hex SHA-1 of
 * `AS+CK+METHOD+URL+BODY+TIMESTAMP`, with the timestamp corrected by `/auth/time`.
 */

export interface OvhCredentials {
  endpoint: string;
  applicationKey: string;
  applicationSecret: string;
  consumerKey: string;
}

export const OVH_US_ENDPOINT = "https://api.us.ovhcloud.com/1.0";

export class OvhError extends Error {
  constructor(
    readonly status: number,
    readonly path: string,
    message: string,
  ) {
    super(`OVH ${status} on ${path}: ${message}`);
  }
}

/** The credentials, or null when this deployment was never given them. */
export function ovhCredentials(env: Env): OvhCredentials | null {
  const applicationKey = env.OVH_APPLICATION_KEY?.trim();
  const applicationSecret = env.OVH_APPLICATION_SECRET?.trim();
  const consumerKey = env.OVH_CONSUMER_KEY?.trim();
  if (!applicationKey || !applicationSecret || !consumerKey) return null;
  return {
    endpoint: (env.OVH_ENDPOINT?.trim() || OVH_US_ENDPOINT).replace(/\/+$/, ""),
    applicationKey,
    applicationSecret,
    consumerKey,
  };
}

async function sha1Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-1", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export async function ovhSignature(
  secret: string,
  consumerKey: string,
  method: string,
  url: string,
  body: string,
  timestamp: number,
): Promise<string> {
  return `$1$${await sha1Hex([secret, consumerKey, method, url, body, String(timestamp)].join("+"))}`;
}

export class OvhClient {
  private drift: number | null = null;

  constructor(
    private readonly creds: OvhCredentials,
    private readonly fetchImpl: typeof fetch = fetch,
    private readonly now: () => number = () => Date.now(),
  ) {}

  private async timestamp(): Promise<number> {
    if (this.drift === null) {
      const res = await this.fetchImpl(`${this.creds.endpoint}/auth/time`);
      if (!res.ok) throw new OvhError(res.status, "/auth/time", "couldn't read OVH's clock");
      const server = Number(await res.text());
      this.drift = Number.isFinite(server) ? server - Math.floor(this.now() / 1000) : 0;
    }
    return Math.floor(this.now() / 1000) + this.drift;
  }

  async call<T>(method: "GET" | "POST", path: string, body?: unknown): Promise<T> {
    const url = `${this.creds.endpoint}${path}`;
    const payload = body === undefined ? "" : JSON.stringify(body);
    const ts = await this.timestamp();
    const signature = await ovhSignature(
      this.creds.applicationSecret,
      this.creds.consumerKey,
      method,
      url,
      payload,
      ts,
    );
    const res = await this.fetchImpl(url, {
      method,
      headers: {
        "Content-Type": "application/json",
        "X-Ovh-Application": this.creds.applicationKey,
        "X-Ovh-Consumer": this.creds.consumerKey,
        "X-Ovh-Timestamp": String(ts),
        "X-Ovh-Signature": signature,
      },
      body: payload || undefined,
    });
    const text = await res.text();
    if (!res.ok) {
      let message = text.slice(0, 300);
      try {
        message = (JSON.parse(text) as { message?: string }).message ?? message;
      } catch {
        // not JSON: keep the text
      }
      throw new OvhError(res.status, path, message);
    }
    return (text ? JSON.parse(text) : null) as T;
  }

  // ---- Ordering -------------------------------------------------------------------------

  /**
   * Order one VPS and pay with the account's default method. Returns the order id and what
   * OVH quoted before checkout. The steps are OVH's documented cart flow.
   */
  async orderVps(input: {
    subsidiary: string;
    planCode: string;
    datacenter: string;
    os: string;
    addons: string[];
  }): Promise<{ orderId: number; price: number | null; currency: string | null }> {
    const cart = await this.call<{ cartId: string }>("POST", "/order/cart", {
      ovhSubsidiary: input.subsidiary,
    });
    const id = encodeURIComponent(cart.cartId);
    await this.call("POST", `/order/cart/${id}/assign`);
    const item = await this.call<{ itemId: number }>("POST", `/order/cart/${id}/vps`, {
      planCode: input.planCode,
      duration: "P1M",
      pricingMode: "default",
      quantity: 1,
    });
    for (const [label, value] of [
      ["vps_datacenter", input.datacenter],
      ["vps_os", input.os],
    ]) {
      await this.call("POST", `/order/cart/${id}/item/${item.itemId}/configuration`, { label, value });
    }
    for (const planCode of input.addons) {
      await this.call("POST", `/order/cart/${id}/vps/options`, {
        itemId: item.itemId,
        planCode,
        duration: "P1M",
        pricingMode: "default",
        quantity: 1,
      });
    }
    const preview = await this.call<OvhOrder>("GET", `/order/cart/${id}/checkout`);
    const order = await this.call<OvhOrder & { orderId: number }>("POST", `/order/cart/${id}/checkout`, {
      autoPayWithPreferredPaymentMethod: true,
      waiveRetractationPeriod: false,
    });
    const price = preview?.prices?.withTax ?? null;
    return { orderId: order.orderId, price: price?.value ?? null, currency: price?.currencyCode ?? null };
  }

  /** The VPS service name an order delivered, or null while it is still on its way. */
  async deliveredService(orderId: number): Promise<string | null> {
    const ids = await this.call<number[]>("GET", `/me/order/${orderId}/details`);
    for (const detailId of ids) {
      const detail = await this.call<{ domain?: string }>("GET", `/me/order/${orderId}/details/${detailId}`);
      // A delivered VPS detail carries the service name; until then it is "*" or empty.
      if (detail.domain && /^vps-[\w.-]+$/.test(detail.domain)) return detail.domain;
    }
    return null;
  }

  async orderStatus(orderId: number): Promise<string> {
    return this.call<string>("GET", `/me/order/${orderId}/status`);
  }

  // ---- A delivered VPS ------------------------------------------------------------------

  async vps(service: string): Promise<{ state: string }> {
    return this.call("GET", `/vps/${encodeURIComponent(service)}`);
  }

  /** The VPS's public IPv4, or null if it has none yet. */
  async ipv4(service: string): Promise<string | null> {
    const ips = await this.call<string[]>("GET", `/vps/${encodeURIComponent(service)}/ips`);
    return ips.find((ip) => /^\d{1,3}(\.\d{1,3}){3}$/.test(ip)) ?? null;
  }

  /** When the monthly subscription renews (ms), from `serviceInfos.expiration`. */
  async renewsAt(service: string): Promise<number | null> {
    const info = await this.call<{ expiration?: string }>(
      "GET",
      `/vps/${encodeURIComponent(service)}/serviceInfos`,
    );
    const at = info.expiration ? Date.parse(info.expiration) : NaN;
    return Number.isFinite(at) ? at : null;
  }

  /** Reinstall with `os` and our public SSH key (the VPS API takes no user data). */
  async rebuild(service: string, os: string, publicSshKey: string): Promise<void> {
    const svc = encodeURIComponent(service);
    const ids = await this.call<string[]>("GET", `/vps/${svc}/images/available`);
    let imageId: string | null = null;
    for (const candidate of ids) {
      const image = await this.call<{ id: string; name: string }>(
        "GET",
        `/vps/${svc}/images/available/${encodeURIComponent(candidate)}`,
      );
      if (image.name === os) {
        imageId = image.id ?? candidate;
        break;
      }
    }
    if (!imageId) throw new OvhError(404, `/vps/${svc}/images/available`, `no ${os} image offered`);
    await this.call("POST", `/vps/${svc}/rebuild`, { imageId, publicSshKey, doNotSendPassword: true });
  }

  /** Whether the VPS still has tasks running (a rebuild in progress). */
  async busy(service: string): Promise<boolean> {
    const tasks = await this.call<number[]>("GET", `/vps/${encodeURIComponent(service)}/tasks?state=todo`);
    const doing = await this.call<number[]>("GET", `/vps/${encodeURIComponent(service)}/tasks?state=doing`);
    return tasks.length + doing.length > 0;
  }
}

interface OvhOrder {
  prices?: { withTax?: { value: number; currencyCode: string } };
}
