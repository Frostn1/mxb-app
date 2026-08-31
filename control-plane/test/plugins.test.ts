import { describe, expect, it } from "vitest";
import {
  ENTITLEMENT_VERSION,
  GRACE_DAYS,
  extendBy,
  myPlugins,
  normaliseCode,
  pluginBundle,
  redeemKey,
  signEntitlement,
  verifyEntitlement,
  type Entitlement,
} from "../src/plugins";

const DAY = 86400;

/** base64url, without reaching for node's Buffer — the worker's own code cannot use it. */
function b64url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function unb64url(s: string): Uint8Array {
  const padded = s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4);
  return Uint8Array.from(atob(padded), (c) => c.charCodeAt(0));
}

/** A real Ed25519 pair, so the signing path is exercised rather than mocked around. */
async function keypair() {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
    "sign",
    "verify",
  ])) as CryptoKeyPair;
  // `exportKey` is typed `ArrayBuffer | JsonWebKey`; "pkcs8" only ever yields the former.
  const pkcs8 = (await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer;
  return { pair, pkcs8B64: b64url(new Uint8Array(pkcs8)) };
}

/**
 * A D1 stand-in that behaves like the real thing on the two points this code leans on:
 * `RETURNING` yields a row only when the UPDATE matched, and the licence upsert replaces.
 */
function stubDb(opts: {
  keys?: Record<string, { plugin_id: string; months: number; redeemed_by: string | null }>;
  licences?: Record<string, number>; // `${account}:${plugin}` -> expires_at
  plugins?: Record<string, { name: string; version: string | null; bundle_sha256: string | null; bundle_key: string | null }>;
} = {}) {
  const keys = opts.keys ?? {};
  const licences = opts.licences ?? {};
  const plugins = opts.plugins ?? {
    replaycam: { name: "Frost's Replay Mod", version: "1.0.0", bundle_sha256: "abc123", bundle_key: "plugins/replaycam-1.0.0.zip" },
  };

  return {
    keys,
    licences,
    prepare(sql: string) {
      return {
        bind(...args: unknown[]) {
          return {
            async first() {
              if (sql.startsWith("UPDATE plugin_keys")) {
                const [account, at, code] = args as [string, number, string];
                const k = keys[code];
                if (!k || k.redeemed_by) return null;
                k.redeemed_by = account;
                void at;
                return { plugin_id: k.plugin_id, months: k.months };
              }
              if (sql.startsWith("SELECT redeemed_by")) {
                const code = String(args[0]);
                return keys[code] ? { redeemed_by: keys[code].redeemed_by } : null;
              }
              if (sql.startsWith("SELECT expires_at")) {
                const [account, plugin] = args as [string, string];
                const e = licences[`${account}:${plugin}`];
                return e ? { expires_at: e } : null;
              }
              if (sql.startsWith("SELECT bundle_sha256")) {
                const p = plugins[String(args[0])];
                return p ? { bundle_sha256: p.bundle_sha256, version: p.version, name: p.name } : null;
              }
              if (sql.startsWith("SELECT p.bundle_key")) {
                const [account, plugin] = args as [string, string];
                const p = plugins[plugin];
                if (!p) return null;
                return {
                  bundle_key: p.bundle_key,
                  bundle_sha256: p.bundle_sha256,
                  expires_at: licences[`${account}:${plugin}`] ?? null,
                };
              }
              return null;
            },
            async all() {
              if (sql.includes("FROM plugin_licences l JOIN plugins p")) {
                const account = String(args[0]);
                const results = Object.entries(licences)
                  .filter(([k]) => k.startsWith(`${account}:`))
                  .map(([k, expires]) => {
                    const plugin = k.split(":")[1];
                    const p = plugins[plugin];
                    return {
                      plugin_id: plugin,
                      expires_at: expires,
                      bundle_sha256: p?.bundle_sha256 ?? null,
                      version: p?.version ?? null,
                      name: p?.name ?? plugin,
                    };
                  });
                return { results };
              }
              return { results: [] };
            },
            async run() {
              if (sql.startsWith("INSERT INTO plugin_licences")) {
                const [account, plugin, expires] = args as [string, string, number];
                licences[`${account}:${plugin}`] = expires;
              }
              return { success: true };
            },
          };
        },
      };
    },
  };
}

function envWith(db: ReturnType<typeof stubDb>, signingKey?: string, r2?: unknown) {
  return { DB: db, PAINTS: r2, PLUGIN_SIGNING_KEY: signingKey } as unknown as Env;
}

const ACCOUNT = { id: "acc_1" };

function post(body: unknown): Request {
  return new Request("https://cp.test/v1/plugins/redeem", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

// ---------------------------------------------------------------------------

describe("entitlement signing", () => {
  it("round-trips through a real Ed25519 signature", async () => {
    const { pair } = await keypair();
    const e: Entitlement = {
      v: ENTITLEMENT_VERSION,
      account: "acc_1",
      plugin: "replaycam",
      expires: 1_800_000_000,
      refreshAfter: 1_700_000_000,
      bundleSha256: "abc123",
      issued: 1_699_000_000,
    };
    const token = await signEntitlement(e, pair.privateKey);
    expect(await verifyEntitlement(token, pair.publicKey)).toEqual(e);
  });

  it("refuses a payload that was edited after signing", async () => {
    const { pair } = await keypair();
    const token = await signEntitlement(
      {
        v: 1,
        account: "acc_1",
        plugin: "replaycam",
        expires: 100,
        refreshAfter: 100,
        bundleSha256: null,
        issued: 0,
      },
      pair.privateKey,
    );
    // Someone extending their own expiry is the exact attack this is here to stop, and it
    // is the easy one to try: the payload is base64'd JSON in plain sight.
    const [payload, sig] = token.split(".");
    const decoded = JSON.parse(new TextDecoder().decode(unb64url(payload)));
    decoded.expires = 9_999_999_999;
    const forged = `${b64url(new TextEncoder().encode(JSON.stringify(decoded)))}.${sig}`;
    expect(await verifyEntitlement(forged, pair.publicKey)).toBeNull();
  });

  it("refuses a signature from a different key", async () => {
    const a = await keypair();
    const b = await keypair();
    const token = await signEntitlement(
      { v: 1, account: "x", plugin: "replaycam", expires: 1, refreshAfter: 1, bundleSha256: null, issued: 0 },
      b.pair.privateKey,
    );
    expect(await verifyEntitlement(token, a.pair.publicKey)).toBeNull();
  });

  it("refuses a token that is not a token", async () => {
    const { pair } = await keypair();
    expect(await verifyEntitlement("nonsense", pair.publicKey)).toBeNull();
  });
});

describe("extendBy", () => {
  const now = Math.floor(Date.UTC(2026, 0, 15) / 1000); // 15 Jan 2026

  it("runs from today when there is no licence", () => {
    const out = extendBy(null, 1, now);
    expect(new Date(out * 1000).toISOString().slice(0, 10)).toBe("2026-02-15");
  });

  it("adds to what is left when renewing early", () => {
    // Renewing on the 15th with a month still to run must land in March, not February —
    // otherwise renewing before you lapse costs you the remainder.
    const current = Math.floor(Date.UTC(2026, 1, 15) / 1000);
    const out = extendBy(current, 1, now);
    expect(new Date(out * 1000).toISOString().slice(0, 10)).toBe("2026-03-15");
  });

  it("runs from today when renewing after a lapse", () => {
    const lapsed = Math.floor(Date.UTC(2025, 10, 1) / 1000);
    const out = extendBy(lapsed, 1, now);
    expect(new Date(out * 1000).toISOString().slice(0, 10)).toBe("2026-02-15");
  });

  it("carries a multi-month key across a year boundary", () => {
    const dec = Math.floor(Date.UTC(2026, 11, 1) / 1000);
    const out = extendBy(null, 3, dec);
    expect(new Date(out * 1000).toISOString().slice(0, 10)).toBe("2027-03-01");
  });
});

describe("redeemKey", () => {
  it("grants a month and returns a verifiable entitlement", async () => {
    const { pair, pkcs8B64 } = await keypair();
    const db = stubDb({ keys: { "FRST-AAAA": { plugin_id: "replaycam", months: 1, redeemed_by: null } } });
    const res = await redeemKey(post({ code: "frst-aaaa" }), ACCOUNT, envWith(db, pkcs8B64));
    expect(res.status).toBe(200);

    const body = (await res.json()) as { plugin: string; expires: number; entitlement: string };
    expect(body.plugin).toBe("replaycam");

    const e = await verifyEntitlement(body.entitlement, pair.publicKey);
    expect(e).not.toBeNull();
    expect(e!.account).toBe("acc_1");
    expect(e!.plugin).toBe("replaycam");
    expect(e!.bundleSha256).toBe("abc123");
    // The grace window, not the subscription: the app has to come back long before the
    // month is out, which is what makes a cancellation take effect.
    expect(e!.refreshAfter - e!.issued).toBeLessThanOrEqual(GRACE_DAYS * DAY);
    expect(e!.expires).toBeGreaterThan(e!.refreshAfter);
  });

  it("never lets refreshAfter outlive the subscription", async () => {
    const { pair, pkcs8B64 } = await keypair();
    // A licence with a day left must not hand out a seven-day grace.
    const nearly = Math.floor(Date.now() / 1000) + DAY;
    const db = stubDb({ licences: { "acc_1:replaycam": nearly } });
    const res = await myPlugins(ACCOUNT, envWith(db, pkcs8B64));
    const body = (await res.json()) as { licences: { entitlement: string }[] };
    const e = await verifyEntitlement(body.licences[0].entitlement, pair.publicKey);
    expect(e!.refreshAfter).toBeLessThanOrEqual(e!.expires);
  });

  it("is one-shot: a second redemption of the same code grants nothing", async () => {
    const { pkcs8B64 } = await keypair();
    const db = stubDb({ keys: { "FRST-AAAA": { plugin_id: "replaycam", months: 1, redeemed_by: null } } });
    const env = envWith(db, pkcs8B64);

    const first = await redeemKey(post({ code: "FRST-AAAA" }), ACCOUNT, env);
    expect(first.status).toBe(200);
    const granted = db.licences["acc_1:replaycam"];

    const second = await redeemKey(post({ code: "FRST-AAAA" }), ACCOUNT, env);
    expect(second.status).toBe(409);
    // And crucially the licence did not move.
    expect(db.licences["acc_1:replaycam"]).toBe(granted);
  });

  it("tells an unknown code apart from a used one", async () => {
    const { pkcs8B64 } = await keypair();
    const db = stubDb({ keys: { "FRST-USED": { plugin_id: "replaycam", months: 1, redeemed_by: "someone" } } });
    const env = envWith(db, pkcs8B64);
    expect((await redeemKey(post({ code: "FRST-USED" }), ACCOUNT, env)).status).toBe(409);
    expect((await redeemKey(post({ code: "FRST-NOPE" }), ACCOUNT, env)).status).toBe(404);
  });

  it("refuses to issue anything when no signing key is configured", async () => {
    const db = stubDb({ keys: { "FRST-AAAA": { plugin_id: "replaycam", months: 1, redeemed_by: null } } });
    const res = await redeemKey(post({ code: "FRST-AAAA" }), ACCOUNT, envWith(db, undefined));
    expect(res.status).toBe(503);
    // An unsigned entitlement is not a degraded one, so nothing may be granted either.
    expect(db.keys["FRST-AAAA"].redeemed_by).toBeNull();
  });

  it("rejects an empty or missing code without touching anything", async () => {
    const { pkcs8B64 } = await keypair();
    const db = stubDb();
    const env = envWith(db, pkcs8B64);
    expect((await redeemKey(post({}), ACCOUNT, env)).status).toBe(400);
    expect((await redeemKey(post({ code: "   " }), ACCOUNT, env)).status).toBe(400);
  });
});

describe("myPlugins", () => {
  it("gives an expired licence no entitlement at all", async () => {
    const { pkcs8B64 } = await keypair();
    const past = Math.floor(Date.now() / 1000) - DAY;
    const db = stubDb({ licences: { "acc_1:replaycam": past } });
    const res = await myPlugins(ACCOUNT, envWith(db, pkcs8B64));
    const body = (await res.json()) as { licences: { active: boolean; entitlement: string | null }[] };
    expect(body.licences[0].active).toBe(false);
    expect(body.licences[0].entitlement).toBeNull();
  });
});

describe("pluginBundle", () => {
  const r2 = { async get(key: string) { return key ? { body: "BUNDLE" } : null; } };

  it("serves the bundle to a live licence", async () => {
    const db = stubDb({ licences: { "acc_1:replaycam": Math.floor(Date.now() / 1000) + DAY } });
    const res = await pluginBundle("replaycam", ACCOUNT, envWith(db, undefined, r2));
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toContain("no-store");
  });

  it("refuses an expired licence", async () => {
    const db = stubDb({ licences: { "acc_1:replaycam": Math.floor(Date.now() / 1000) - DAY } });
    expect((await pluginBundle("replaycam", ACCOUNT, envWith(db, undefined, r2))).status).toBe(403);
  });

  it("refuses an account with no licence", async () => {
    const db = stubDb();
    expect((await pluginBundle("replaycam", ACCOUNT, envWith(db, undefined, r2))).status).toBe(403);
  });

  it("404s a plugin that has no build published", async () => {
    const db = stubDb({
      licences: { "acc_1:replaycam": Math.floor(Date.now() / 1000) + DAY },
      plugins: { replaycam: { name: "x", version: null, bundle_sha256: null, bundle_key: null } },
    });
    expect((await pluginBundle("replaycam", ACCOUNT, envWith(db, undefined, r2))).status).toBe(404);
  });
});

describe("normaliseCode", () => {
  it("accepts the shapes a human types", () => {
    expect(normaliseCode(" frst-aaaa-bbbb ")).toBe("FRST-AAAA-BBBB");
    expect(normaliseCode("FRST AAAA BBBB")).toBe("FRSTAAAABBBB");
    expect(normaliseCode("frst‑aaaa")).toBe("FRSTAAAA"); // a pasted en-dash is not a hyphen
  });
});
