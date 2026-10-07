import { describe, expect, it } from "vitest";
import {
  FREE_TERM_DAYS,
  GRACE_DAYS,
  LICENSE_VERSION,
  listPlugins,
  myPlugins,
  pluginBundle,
  signLicense,
  verifyLicense,
  type License,
} from "../src/plugins";
import { addAccount, d1, publishBundle } from "./d1sqlite";

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

const r2 = {
  async get(key: string) {
    return key ? { body: "BUNDLE" } : null;
  },
};

/** A deployment with a database, a signing key and a published build. */
async function deployment(opts: { signing?: boolean } = {}) {
  const { pair, pkcs8B64 } = await keypair();
  const DB = d1();
  await addAccount(DB, "acc_1", "Frost", "76561198000000001");
  await publishBundle(DB, "replaycam", "1.0.0", "abc123");
  const env = {
    DB,
    PAINTS: r2,
    PLUGIN_SIGNING_KEY: opts.signing === false ? undefined : pkcs8B64,
  } as unknown as Env;
  return { env, DB, pair };
}

const ACCOUNT = { id: "acc_1" };

// ---------------------------------------------------------------------------

describe("every plugin is free", () => {
  it("gives every account a verifiable license with no key", async () => {
    const { env, pair } = await deployment();
    const res = await myPlugins(ACCOUNT, env);
    const body = (await res.json()) as {
      licenses: { plugin: string; active: boolean; free: boolean; expires: number; license: string | null }[];
    };
    expect(body.licenses).toHaveLength(1);
    const row = body.licenses[0];
    expect(row).toMatchObject({ plugin: "replaycam", active: true, free: true });
    const lic = await verifyLicense(row.license!, pair.publicKey);
    expect(lic?.account).toBe("acc_1");
    expect(lic?.bundleSha256).toBe("abc123");
    expect(lic!.refreshAfter - lic!.issued).toBeLessThanOrEqual(GRACE_DAYS * DAY);
    expect(lic!.expires - lic!.issued).toBeGreaterThanOrEqual(FREE_TERM_DAYS * DAY - 5);
  });

  it("treats a plugin as free even if its row says otherwise", async () => {
    const { env } = await deployment();
    await env.DB.prepare(`UPDATE plugins SET free = 0`).run();
    const mine = (await (await myPlugins(ACCOUNT, env)).json()) as { licenses: { free: boolean; license: string | null }[] };
    expect(mine.licenses[0]).toMatchObject({ free: true });
    expect(mine.licenses[0].license).not.toBeNull();
    const list = (await (await listPlugins(env)).json()) as { plugins: { free: boolean }[] };
    expect(list.plugins.every((p) => p.free)).toBe(true);
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(200);
  });

  it("answers 503 rather than issuing an unsigned license", async () => {
    const { env } = await deployment({ signing: false });
    expect((await myPlugins(ACCOUNT, env)).status).toBe(503);
  });

  it("has no key or license tables once the migrations have run", async () => {
    const DB = d1();
    const { results } = await DB.prepare(
      `SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('plugin_keys', 'plugin_licenses')`,
    ).all();
    expect(results).toEqual([]);
    const row = await DB.prepare(`SELECT name, free FROM plugins WHERE id = 'replaycam'`).first();
    expect(row).toEqual({ name: "MXB Replay", free: 1 });
  });
});

describe("license signing", () => {
  it("round-trips through a real Ed25519 signature", async () => {
    const { pair } = await keypair();
    const e: License = {
      v: LICENSE_VERSION,
      account: "acc_1",
      plugin: "replaycam",
      expires: 1_800_000_000,
      refreshAfter: 1_700_000_000,
      bundleSha256: "abc123",
      issued: 1_699_000_000,
    };
    const token = await signLicense(e, pair.privateKey);
    expect(await verifyLicense(token, pair.publicKey)).toEqual(e);
  });

  it("refuses a payload that was edited after signing", async () => {
    const { pair } = await keypair();
    const token = await signLicense(
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
    expect(await verifyLicense(forged, pair.publicKey)).toBeNull();
  });

  it("refuses a signature from a different key", async () => {
    const a = await keypair();
    const b = await keypair();
    const token = await signLicense(
      { v: 1, account: "x", plugin: "replaycam", expires: 1, refreshAfter: 1, bundleSha256: null, issued: 0 },
      b.pair.privateKey,
    );
    expect(await verifyLicense(token, a.pair.publicKey)).toBeNull();
  });

  it("refuses a token that is not a token", async () => {
    const { pair } = await keypair();
    expect(await verifyLicense("nonsense", pair.publicKey)).toBeNull();
  });
});

describe("pluginBundle", () => {
  it("serves the bundle to a signed-in account", async () => {
    const { env } = await deployment();
    const res = await pluginBundle("replaycam", ACCOUNT, env);
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toContain("no-store");
  });

  it("404s an unknown plugin, and one that has no build published", async () => {
    const { env } = await deployment();
    expect((await pluginBundle("nope", ACCOUNT, env)).status).toBe(404);
    await env.DB.prepare(`UPDATE plugins SET bundle_key = NULL WHERE id = 'replaycam'`).run();
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(404);
  });
});
