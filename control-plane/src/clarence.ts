/**
 * Website orders into DGS Clarence: the Discord ticket bot the swap creators already work from.
 *
 * Out: when a rider sends a build, it is posted to Clarence (`CLARENCE_ORDERS_URL`), which opens a
 * private Model Swap ticket for it. Again when the rider answers an approval request on the site.
 * In: Clarence posts back to `/v1/clarence/builds/:id` as the ticket moves (quote, payment,
 * approval, complete), so the rider's page and the ticket say the same thing.
 *
 * Both ways are signed like Stripe's webhooks: `X-MXB-Signature: t=<unix>,v1=<hex>`, HMAC-SHA256
 * of `<t>.<raw body>` under `CLARENCE_SECRET`, `t` within five minutes. Off (and the inbound route
 * 503) unless both are set. The contract is docs/clarence.md.
 */

import type { Spec } from "./builds";

export const SIGNATURE_HEADER = "X-MXB-Signature";
const TOLERANCE_S = 300;
const SEND_TIMEOUT_MS = 8000;

export interface ClarenceConfig {
  url: string;
  secret: string;
}

export function clarenceConfig(env: Env): ClarenceConfig | null {
  const url = env.CLARENCE_ORDERS_URL?.trim();
  const secret = env.CLARENCE_SECRET?.trim();
  if (!url || !secret || !url.startsWith("https://")) return null;
  return { url, secret };
}

async function hmacHex(secret: string, message: string): Promise<string> {
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(message)));
  return Array.from(mac, (b) => b.toString(16).padStart(2, "0")).join("");
}

export async function sign(secret: string, body: string, nowMs = Date.now()): Promise<string> {
  const t = Math.floor(nowMs / 1000);
  return `t=${t},v1=${await hmacHex(secret, `${t}.${body}`)}`;
}

/** Constant-time over equal lengths. */
function same(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}

export async function verify(secret: string, body: string, header: string | null, nowMs = Date.now()): Promise<boolean> {
  if (!header) return false;
  const parts = Object.fromEntries(header.split(",").map((p) => p.trim().split("=", 2) as [string, string]));
  const t = Number(parts.t);
  if (!Number.isInteger(t) || !parts.v1 || Math.abs(nowMs / 1000 - t) > TOLERANCE_S) return false;
  return same(parts.v1, await hmacHex(secret, `${t}.${body}`));
}

/** What Clarence gets for an order: everything the ticket needs, nothing about where files live. */
export interface OrderEvent {
  event: "order.created" | "order.approval";
  id: string;
  ref: string;
  /** The rider: Steam sign-in, and the Discord name they gave for the ticket. */
  steamId: string;
  steamName: string;
  discord: string | null;
  guids: number;
  note: string | null;
  stage: string;
  priceCents: number | null;
  approval: string | null;
  approvalNote: string | null;
  spec: Spec;
  /** Where the rider follows the order. */
  page: string;
  sentAt: number | null;
}

/** Post one event. True when Clarence took it (2xx). Never throws. */
export async function postToClarence(cfg: ClarenceConfig, ev: OrderEvent, fetchImpl: typeof fetch = fetch, nowMs = Date.now()): Promise<boolean> {
  const body = JSON.stringify(ev);
  try {
    const res = await fetchImpl(cfg.url, {
      method: "POST",
      headers: { "Content-Type": "application/json", [SIGNATURE_HEADER]: await sign(cfg.secret, body, nowMs) },
      body,
      signal: AbortSignal.timeout(SEND_TIMEOUT_MS),
    });
    if (!res.ok) console.log(JSON.stringify({ msg: "clarence refused", id: ev.id, status: res.status }));
    return res.ok;
  } catch (e) {
    console.log(JSON.stringify({ msg: "clarence unreachable", id: ev.id, error: String(e) }));
    return false;
  }
}

/** A Discord channel link, the only kind of ticket link kept. */
export function ticketUrl(v: unknown): string | null {
  return typeof v === "string" && /^https:\/\/discord\.com\/channels\/\d{5,25}\/\d{5,25}$/.test(v) ? v : null;
}

/** A Discord username as typed: 2-32 of lower-case letters, digits, `_` and `.`. */
export function discordName(v: unknown): string | null | false {
  if (v === undefined || v === null || v === "") return null;
  if (typeof v !== "string") return false;
  const s = v.trim().replace(/^@/, "").toLowerCase();
  return /^[a-z0-9_.]{2,32}$/.test(s) ? s : false;
}
