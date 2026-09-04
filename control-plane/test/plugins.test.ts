import { describe, expect, it } from "vitest";
import {
  LICENSE_VERSION,
  GRACE_DAYS,
  extendBy,
  grantLicense,
  keyState,
  licenseState,
  MAX_MINT,
  MAX_MONTHS,
  mintKeys,
  myPlugins,
  newCode,
  normaliseCode,
  pluginBundle,
  redeemKey,
  searchKeys,
  searchLicenses,
  setKeyRevoked,
  setLicenseRevoked,
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

/**
 * A deployment with a database, a signing key and a published build — the state everything
 * below starts from, because a control plane missing any of the three has its own tests.
 */
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

/** One key for the plugin under test, and the code that redeems it. */
async function mintOne(env: Env, months = 1): Promise<string> {
  const minted = await mintKeys(env, "replaycam", months, 1, "test");
  expect(minted.ok).toBe(true);
  return minted.codes[0];
}

async function licenseRow(env: Env, account = "acc_1") {
  return env.DB.prepare(
    `SELECT expires_at, revoked_at FROM plugin_licenses WHERE account_id = ? AND plugin_id = ?`,
  )
    .bind(account, "replaycam")
    .first<{ expires_at: number; revoked_at: number | null }>();
}

/** A license with an expiry the test picks, for the windows nothing else can produce. */
async function grantMonths(env: Env, expiresAt: number, account = "acc_1"): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO plugin_licenses (account_id, plugin_id, expires_at, granted_at)
     VALUES (?, 'replaycam', ?, unixepoch())
     ON CONFLICT (account_id, plugin_id) DO UPDATE SET expires_at = excluded.expires_at`,
  )
    .bind(account, expiresAt)
    .run();
}

async function keyRow(env: Env, code: string) {
  return env.DB.prepare(`SELECT redeemed_by, revoked_at FROM plugin_keys WHERE code = ?`)
    .bind(code)
    .first<{ redeemed_by: string | null; revoked_at: number | null }>();
}

const ACCOUNT = { id: "acc_1" };

function post(body: unknown): Request {
  return new Request("https://cp.test/v1/plugins/redeem", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

// ---------------------------------------------------------------------------

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

describe("extendBy", () => {
  const now = Math.floor(Date.UTC(2026, 0, 15) / 1000); // 15 Jan 2026

  it("runs from today when there is no license", () => {
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
  it("grants a month and returns a verifiable license", async () => {
    const { env, pair } = await deployment();
    const code = await mintOne(env);
    const res = await redeemKey(post({ code: code.toLowerCase() }), ACCOUNT, env);
    expect(res.status).toBe(200);

    const body = (await res.json()) as { plugin: string; expires: number; license: string };
    expect(body.plugin).toBe("replaycam");

    const e = await verifyLicense(body.license, pair.publicKey);
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
    const { env, pair } = await deployment();
    // A license with a day left must not hand out a seven-day grace.
    await grantMonths(env, Math.floor(Date.now() / 1000) + DAY);
    const res = await myPlugins(ACCOUNT, env);
    const body = (await res.json()) as { licenses: { license: string }[] };
    const e = await verifyLicense(body.licenses[0].license, pair.publicKey);
    expect(e!.refreshAfter).toBeLessThanOrEqual(e!.expires);
  });

  it("is one-shot: a second redemption of the same code grants nothing", async () => {
    const { env } = await deployment();
    const code = await mintOne(env);

    expect((await redeemKey(post({ code }), ACCOUNT, env)).status).toBe(200);
    const granted = (await licenseRow(env))!.expires_at;

    const second = await redeemKey(post({ code }), ACCOUNT, env);
    expect(second.status).toBe(409);
    // And crucially the license did not move.
    expect((await licenseRow(env))!.expires_at).toBe(granted);
  });

  it("tells an unknown code apart from a used one", async () => {
    const { env } = await deployment();
    const code = await mintOne(env);
    expect((await redeemKey(post({ code }), ACCOUNT, env)).status).toBe(200);
    expect((await redeemKey(post({ code }), ACCOUNT, env)).status).toBe(409);
    expect((await redeemKey(post({ code: "FRST-NOPE" }), ACCOUNT, env)).status).toBe(404);
  });

  it("refuses to issue anything when no signing key is configured", async () => {
    const { env } = await deployment({ signing: false });
    // Minting does not need the signing key, only redeeming does.
    const code = await mintOne(env);
    const res = await redeemKey(post({ code }), ACCOUNT, env);
    expect(res.status).toBe(503);
    // An unsigned license is not a degraded one, so nothing may be granted either.
    expect((await keyRow(env, code))!.redeemed_by).toBeNull();
  });

  it("rejects an empty or missing code without touching anything", async () => {
    const { env } = await deployment();
    expect((await redeemKey(post({}), ACCOUNT, env)).status).toBe(400);
    expect((await redeemKey(post({ code: "   " }), ACCOUNT, env)).status).toBe(400);
  });
});

describe("revoking a key", () => {
  it("stops it being redeemed, and says which kind of no it is", async () => {
    const { env } = await deployment();
    const code = await mintOne(env);
    await setKeyRevoked(env, code, true);

    const res = await redeemKey(post({ code }), ACCOUNT, env);
    // Not 404 and not 409: whoever is holding this code needs to know it was withdrawn
    // rather than mistyped or already spent.
    expect(res.status).toBe(403);
    expect(await licenseRow(env)).toBeNull();
    // And it is not quietly marked as spent by the attempt.
    expect((await keyRow(env, code))!.redeemed_by).toBeNull();
  });

  it("can be undone, and the code works again", async () => {
    const { env } = await deployment();
    const code = await mintOne(env);
    await setKeyRevoked(env, code, true);
    await setKeyRevoked(env, code, false);
    expect((await redeemKey(post({ code }), ACCOUNT, env)).status).toBe(200);
  });

  it("takes the code however it is typed back", async () => {
    const { env } = await deployment();
    const code = await mintOne(env);
    await setKeyRevoked(env, ` ${code.toLowerCase()} `, true);
    expect((await keyRow(env, code))!.revoked_at).toBeTruthy();
  });
});

describe("revoking a license", () => {
  it("takes the plugin away without waiting for the month to end", async () => {
    const { env } = await deployment();
    expect((await redeemKey(post({ code: await mintOne(env) }), ACCOUNT, env)).status).toBe(200);
    await setLicenseRevoked(env, "acc_1", "replaycam", true);

    // Gone from what the app is told it holds, rather than present and inactive: a revoked
    // license is reported the same way as one that was never bought.
    const mine = (await (await myPlugins(ACCOUNT, env)).json()) as { licenses: unknown[] };
    expect(mine.licenses).toHaveLength(0);
    // And the bundle route says no on its own, without consulting the first.
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(403);
  });

  it("keeps the months that were paid for, so lifting it gives them back", async () => {
    const { env } = await deployment();
    await redeemKey(post({ code: await mintOne(env, 3) }), ACCOUNT, env);
    const before = (await licenseRow(env))!.expires_at;

    await setLicenseRevoked(env, "acc_1", "replaycam", true);
    await setLicenseRevoked(env, "acc_1", "replaycam", false);

    const after = await licenseRow(env);
    expect(after!.revoked_at).toBeNull();
    expect(after!.expires_at).toBe(before);
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(200);
  });

  it("refuses to spend another key on it, rather than burning the code", async () => {
    const { env } = await deployment();
    await redeemKey(post({ code: await mintOne(env) }), ACCOUNT, env);
    await setLicenseRevoked(env, "acc_1", "replaycam", true);

    const code = await mintOne(env);
    const res = await redeemKey(post({ code }), ACCOUNT, env);
    expect(res.status).toBe(403);
    // The person redeeming it did nothing wrong, and gets to keep the key.
    expect((await keyRow(env, code))!.redeemed_by).toBeNull();
  });
});

describe("mintKeys", () => {
  it("mints a batch that is redeemable and unique", async () => {
    const { env } = await deployment();
    const minted = await mintKeys(env, "replaycam", 1, 25, "August giveaway");
    expect(minted.ok).toBe(true);
    expect(new Set(minted.codes).size).toBe(25);
    expect(minted.codes.every((c) => c === normaliseCode(c))).toBe(true);
    expect((await redeemKey(post({ code: minted.codes[7] }), ACCOUNT, env)).status).toBe(200);
  });

  it("mints a batch under one timestamp, which is how the page finds it again", async () => {
    const { env } = await deployment();
    await mintKeys(env, "replaycam", 1, 5, "batch");
    const stamps = await env.DB.prepare(
      `SELECT DISTINCT created_at FROM plugin_keys`,
    ).all<{ created_at: number }>();
    expect(stamps.results).toHaveLength(1);
  });

  it("refuses months and counts nobody meant to type", async () => {
    const { env } = await deployment();
    for (const [months, count] of [
      [0, 1],
      [MAX_MONTHS + 1, 1],
      [1.5, 1],
      [1, 0],
      [1, MAX_MINT + 1],
    ]) {
      expect((await mintKeys(env, "replaycam", months, count, "")).ok).toBe(false);
    }
    expect((await mintKeys(env, "no-such-plugin", 1, 1, "")).ok).toBe(false);
    const left = await env.DB.prepare(`SELECT COUNT(*) AS n FROM plugin_keys`).first<{ n: number }>();
    expect(left!.n).toBe(0);
  });

  it("writes a note that survives a quote in it", async () => {
    const { env } = await deployment();
    const minted = await mintKeys(env, "replaycam", 1, 1, "Frost's testers");
    const row = await env.DB.prepare(`SELECT note FROM plugin_keys WHERE code = ?`)
      .bind(minted.codes[0])
      .first<{ note: string }>();
    expect(row!.note).toBe("Frost's testers");
  });
});

describe("newCode", () => {
  it("is the shape a person types off a Discord message", () => {
    for (let i = 0; i < 200; i++) {
      const code = newCode();
      expect(code).toMatch(/^FRST-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/);
      // The letters that get misread aloud or in a screenshot are not in the alphabet.
      expect(code.slice(5)).not.toMatch(/[ILOU]/);
    }
  });
});

describe("grantLicense", () => {
  it("gives months with no key in between", async () => {
    const { env } = await deployment();
    const granted = await grantLicense(env, "acc_1", "replaycam", 1);
    expect(granted.ok).toBe(true);
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(200);
  });

  it("finds the person however they are named", async () => {
    const { env } = await deployment();
    expect((await grantLicense(env, "frost", "replaycam", 1)).account).toBe("acc_1");
    expect((await grantLicense(env, "76561198000000001", "replaycam", 1)).account).toBe("acc_1");
    expect((await grantLicense(env, "nobody", "replaycam", 1)).ok).toBe(false);
  });

  it("adds to what is left rather than restarting it", async () => {
    const { env } = await deployment();
    const first = await grantLicense(env, "acc_1", "replaycam", 1);
    const second = await grantLicense(env, "acc_1", "replaycam", 1);
    expect(second.expires!).toBeGreaterThan(first.expires!);
  });

  it("will not put months on a revoked license", async () => {
    const { env } = await deployment();
    await grantLicense(env, "acc_1", "replaycam", 1);
    await setLicenseRevoked(env, "acc_1", "replaycam", true);
    const again = await grantLicense(env, "acc_1", "replaycam", 1);
    expect(again.ok).toBe(false);
    // Silently adding them would look like it worked and change nothing the app can see.
    expect((await licenseRow(env))!.revoked_at).toBeTruthy();
  });

  it("refuses months nobody meant to type, and an account that is not one", async () => {
    const { env } = await deployment();
    expect((await grantLicense(env, "acc_1", "replaycam", 0)).ok).toBe(false);
    expect((await grantLicense(env, "acc_1", "replaycam", MAX_MONTHS + 1)).ok).toBe(false);
    expect((await grantLicense(env, "acc_1", "nope", 1)).ok).toBe(false);
  });
});

describe("the admin lists", () => {
  it("counts and filters keys by what became of them", async () => {
    const { env } = await deployment();
    const minted = await mintKeys(env, "replaycam", 1, 4, "batch");
    await redeemKey(post({ code: minted.codes[0] }), ACCOUNT, env);
    await setKeyRevoked(env, minted.codes[1], true);

    const any = await searchKeys(env, { q: "", plugin: "", state: "any", page: 1 });
    expect(any.total).toBe(4);
    const unused = await searchKeys(env, { q: "", plugin: "", state: "unused", page: 1 });
    expect(unused.rows.map((r) => r.code)).toEqual(
      expect.arrayContaining([minted.codes[2], minted.codes[3]]),
    );
    expect(unused.total).toBe(2);
    const redeemed = await searchKeys(env, { q: "", plugin: "", state: "redeemed", page: 1 });
    expect(redeemed.rows).toHaveLength(1);
    // The account that spent it is resolved to the name it rides under.
    expect(redeemed.rows[0].rider_name).toBe("Frost");
    const revoked = await searchKeys(env, { q: "", plugin: "", state: "revoked", page: 1 });
    expect(revoked.rows.map((r) => r.code)).toEqual([minted.codes[1]]);
  });

  it("searches keys by code, by note and by who spent one", async () => {
    const { env } = await deployment();
    const minted = await mintKeys(env, "replaycam", 1, 2, "August giveaway");
    await redeemKey(post({ code: minted.codes[0] }), ACCOUNT, env);

    const byNote = await searchKeys(env, { q: "august", plugin: "", state: "any", page: 1 });
    expect(byNote.total).toBe(2);
    const byCode = await searchKeys(env, { q: minted.codes[1], plugin: "", state: "any", page: 1 });
    expect(byCode.total).toBe(1);
    const byRider = await searchKeys(env, { q: "Frost", plugin: "", state: "any", page: 1 });
    expect(byRider.rows.map((r) => r.code)).toEqual([minted.codes[0]]);
    const byNothing = await searchKeys(env, { q: "zzz", plugin: "", state: "any", page: 1 });
    expect(byNothing.total).toBe(0);
  });

  it("keeps a typed underscore an underscore", async () => {
    const { env } = await deployment();
    await mintKeys(env, "replaycam", 1, 1, "a_b");
    await mintKeys(env, "replaycam", 1, 1, "axb");
    const found = await searchKeys(env, { q: "a_b", plugin: "", state: "any", page: 1 });
    expect(found.total).toBe(1);
  });

  it("separates a live license from an expired one and a revoked one", async () => {
    const { env } = await deployment();
    await addAccount(env.DB, "acc_2", "Ripper");
    await addAccount(env.DB, "acc_3", "Lapsed");
    await grantLicense(env, "acc_1", "replaycam", 1);
    await grantLicense(env, "acc_2", "replaycam", 1);
    await setLicenseRevoked(env, "acc_2", "replaycam", true);
    await grantMonths(env, Math.floor(Date.now() / 1000) - DAY, "acc_3");

    const live = await searchLicenses(env, { q: "", plugin: "", state: "live", page: 1 });
    expect(live.rows.map((r) => r.rider_name)).toEqual(["Frost"]);
    const expired = await searchLicenses(env, { q: "", plugin: "", state: "expired", page: 1 });
    expect(expired.rows.map((r) => r.rider_name)).toEqual(["Lapsed"]);
    const revoked = await searchLicenses(env, { q: "", plugin: "", state: "revoked", page: 1 });
    expect(revoked.rows.map((r) => r.rider_name)).toEqual(["Ripper"]);
    const all = await searchLicenses(env, { q: "", plugin: "", state: "any", page: 1 });
    expect(all.total).toBe(3);
  });

  it("finds a license by rider name, account id or Steam id", async () => {
    const { env } = await deployment();
    await grantLicense(env, "acc_1", "replaycam", 1);
    for (const q of ["frost", "acc_1", "765611980"]) {
      const found = await searchLicenses(env, { q, plugin: "", state: "any", page: 1 });
      expect(found.total, q).toBe(1);
    }
  });

  it("filters both lists by plugin", async () => {
    const { env } = await deployment();
    await mintKeys(env, "replaycam", 1, 2, "");
    await grantLicense(env, "acc_1", "replaycam", 1);
    expect((await searchKeys(env, { q: "", plugin: "replaycam", state: "any", page: 1 })).total).toBe(2);
    expect((await searchKeys(env, { q: "", plugin: "other", state: "any", page: 1 })).total).toBe(0);
    expect(
      (await searchLicenses(env, { q: "", plugin: "replaycam", state: "any", page: 1 })).total,
    ).toBe(1);
    expect((await searchLicenses(env, { q: "", plugin: "other", state: "any", page: 1 })).total).toBe(0);
  });

  it("pages, and page two is the rest of it", async () => {
    const { env } = await deployment();
    await mintKeys(env, "replaycam", 1, 60, "");
    const one = await searchKeys(env, { q: "", plugin: "", state: "any", page: 1 });
    const two = await searchKeys(env, { q: "", plugin: "", state: "any", page: 2 });
    expect(one.rows).toHaveLength(50);
    expect(two.rows).toHaveLength(10);
    expect(one.total).toBe(60);
    // No code appears on both pages, which is what an unstable sort would give.
    expect(new Set([...one.rows, ...two.rows].map((r) => r.code)).size).toBe(60);
  });
});

describe("naming the state of things", () => {
  it("lets revoked win over everything else", () => {
    const now = Math.floor(Date.now() / 1000);
    expect(keyState({ redeemed_by: null, revoked_at: null })).toBe("unused");
    expect(keyState({ redeemed_by: "acc_1", revoked_at: null })).toBe("redeemed");
    expect(keyState({ redeemed_by: "acc_1", revoked_at: now })).toBe("revoked");
    expect(licenseState({ expires_at: now + DAY, revoked_at: null }, now)).toBe("live");
    expect(licenseState({ expires_at: now - DAY, revoked_at: null }, now)).toBe("expired");
    expect(licenseState({ expires_at: now + DAY, revoked_at: now }, now)).toBe("revoked");
  });
});

describe("myPlugins", () => {
  it("gives an expired license no license at all", async () => {
    const { env } = await deployment();
    await grantMonths(env, Math.floor(Date.now() / 1000) - DAY);
    const res = await myPlugins(ACCOUNT, env);
    const body = (await res.json()) as { licenses: { active: boolean; license: string | null }[] };
    expect(body.licenses[0].active).toBe(false);
    expect(body.licenses[0].license).toBeNull();
  });
});

describe("pluginBundle", () => {
  it("serves the bundle to a live license", async () => {
    const { env } = await deployment();
    await grantLicense(env, "acc_1", "replaycam", 1);
    const res = await pluginBundle("replaycam", ACCOUNT, env);
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toContain("no-store");
  });

  it("refuses an expired license", async () => {
    const { env } = await deployment();
    await grantMonths(env, Math.floor(Date.now() / 1000) - DAY);
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(403);
  });

  it("refuses an account with no license", async () => {
    const { env } = await deployment();
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(403);
  });

  it("404s a plugin that has no build published", async () => {
    const { env } = await deployment();
    await env.DB.prepare(`UPDATE plugins SET bundle_key = NULL WHERE id = 'replaycam'`).run();
    await grantLicense(env, "acc_1", "replaycam", 1);
    expect((await pluginBundle("replaycam", ACCOUNT, env)).status).toBe(404);
  });
});

describe("normaliseCode", () => {
  it("accepts the shapes a human types", () => {
    expect(normaliseCode(" frst-aaaa-bbbb ")).toBe("FRST-AAAA-BBBB");
    expect(normaliseCode("FRST AAAA BBBB")).toBe("FRSTAAAABBBB");
    expect(normaliseCode("frst‑aaaa")).toBe("FRSTAAAA"); // a pasted en-dash is not a hyphen
  });
});
