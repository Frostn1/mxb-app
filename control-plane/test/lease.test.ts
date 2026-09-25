/**
 * Key leases (`src/lease.ts`): the signed, 30-day permission a `.mxbkey` needs beside it before
 * the DLL will unseal it, and the renewal a ban stops.
 */
import { describe, expect, it, vi } from "vitest";
import { APP_BLOCK_CODE, APP_BLOCK_MESSAGE, addBan } from "../src/bans";
import { hashToken } from "../src/auth";
import { guidFromSteamId } from "../src/steam";
import { LEASE_PURPOSE, LEASE_TTL_MS, signLeaseWith, verifyLease, type SignedLease } from "../src/lease";
import { signVerdict, verifyVerdict } from "../src/verdict";
import { addAccount, d1 } from "./d1sqlite";

// The entry module exports the voice Durable Object, whose base class only exists in workerd.
vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const OWNER = "acc_owner";
const BOSS = "76561198174305985";
const BUYER = "76561198000000042";
const CLEAN = "76561198000000077";
/** Synthetic, and not one of the GUIDs the deployment ships banned. */
const GUID = "AA0110000100000001";
const OTHER_GUID = "AA0110000100000002";

/** A fresh pair, the private half in the shape `MXB_VERDICT_SIGNING_KEY` holds. */
async function signingPair(): Promise<{ secret: string; publicKey: CryptoKey; privateKey: CryptoKey }> {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])) as CryptoKeyPair;
  const pkcs8 = new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer);
  let binary = "";
  for (const b of pkcs8) binary += String.fromCharCode(b);
  const secret = btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return { secret, publicKey: pair.publicKey, privateKey: pair.privateKey };
}

async function deployment(overrides: Record<string, string> = {}): Promise<Env> {
  const DB = d1();
  await addAccount(DB, OWNER, "Owner");
  return { DB, MXB_OWNER_ACCOUNT_ID: OWNER, MXB_ADMIN_STEAM_IDS: BOSS, ...overrides } as unknown as Env;
}

/** An app account with a bearer token, the way every client arrives. */
async function account(env: Env, id: string, token: string, steamId: string | null, guid: string | null) {
  await env.DB.prepare(
    "INSERT INTO accounts (id, rider_name, steam_id, guid, token_hash, created_at) VALUES (?, ?, ?, ?, ?, ?)",
  )
    .bind(id, id, steamId, guid, await hashToken(token), Date.now())
    .run();
}

const lease = (env: Env, token: string) =>
  worker.fetch(
    new Request("https://cp.test/v1/keys/lease", { method: "POST", headers: { Authorization: `Bearer ${token}` } }),
    env,
    {} as ExecutionContext,
  );

describe("POST /v1/keys/lease", () => {
  it("leases a clean, Steam-linked account for 30 days, signed and tagged", async () => {
    const { secret, publicKey } = await signingPair();
    const env = await deployment({ MXB_VERDICT_SIGNING_KEY: secret });
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);

    const before = Date.now();
    const res = await lease(env, "clean-token");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { lease: SignedLease; expiresAt: number };
    const payload = await verifyLease(body.lease, publicKey);
    expect(payload).not.toBeNull();
    expect(payload!.purpose).toBe(LEASE_PURPOSE);
    expect(payload!.purpose).toBe("mxbsecure-lease");
    expect(payload!.v).toBe(1);
    expect(payload!.steamId).toBe(CLEAN);
    expect(payload!.issuedAt).toBeGreaterThanOrEqual(before);
    expect(payload!.expiresAt - payload!.issuedAt).toBe(30 * 24 * 60 * 60 * 1000);
    expect(body.expiresAt).toBe(payload!.expiresAt);
    // The wire order the DLL reads, and nothing about the account beyond its Steam ID.
    expect(Object.keys(JSON.parse(body.lease.payload))).toEqual(["v", "purpose", "steamId", "issuedAt", "expiresAt"]);
  });

  it("refuses a banned account with the block code, so the app drops its lease", async () => {
    const { secret } = await signingPair();
    const env = await deployment({ MXB_VERDICT_SIGNING_KEY: secret });
    await account(env, "acc_banned", "banned-token", BUYER, GUID);
    await addBan(env, { guid: GUID, reason: "unlocked and shared protected content" }, BOSS);

    const res = await lease(env, "banned-token");
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: APP_BLOCK_MESSAGE, code: APP_BLOCK_CODE });
  });

  it("refuses a banned Steam ID even when the account row has not caught up with the link", async () => {
    const { secret } = await signingPair();
    const env = await deployment({ MXB_VERDICT_SIGNING_KEY: secret });
    // No Steam ID on the row (so the door's ban check sees none), but a confirmed link to a
    // banned one — which is what `steamIdFor` resolves and the lease would be issued for.
    await account(env, "acc_linked", "linked-token", null, null);
    await env.DB.prepare("INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES (?, ?, ?)")
      .bind("acc_linked", BUYER, Date.now())
      .run();
    // A Steam copy's GUID is its Steam ID in hex, so banning that GUID bans the Steam login.
    await addBan(env, { guid: guidFromSteamId(BUYER), reason: "shared keys" }, BOSS);

    const res = await lease(env, "linked-token");
    expect(res.status).toBe(403);
    expect(await res.json()).toMatchObject({ code: APP_BLOCK_CODE });
  });

  it("refuses an account with no Steam link, in the grant's own words", async () => {
    const { secret } = await signingPair();
    const env = await deployment({ MXB_VERDICT_SIGNING_KEY: secret });
    await account(env, "acc_new", "new-token", null, null);

    const res = await lease(env, "new-token");
    expect(res.status).toBe(409);
    expect(await res.json()).toEqual({ error: "no Steam account linked" });
  });

  it("answers 503 when the deployment has no signing key", async () => {
    const env = await deployment();
    await account(env, "acc_clean", "clean-token", CLEAN, OTHER_GUID);
    const res = await lease(env, "clean-token");
    expect(res.status).toBe(503);
    expect(await res.json()).toEqual({ error: "leases not configured" });
  });

  it("needs a bearer token", async () => {
    const env = await deployment();
    const res = await worker.fetch(
      new Request("https://cp.test/v1/keys/lease", { method: "POST" }),
      env,
      {} as ExecutionContext,
    );
    expect(res.status).toBe(401);
  });
});

describe("the lease signature", () => {
  it("refuses a tampered lease and one signed by another key", async () => {
    const { publicKey, privateKey } = await signingPair();
    const signed = await signLeaseWith(CLEAN, privateKey, 1_700_000_000_000);
    expect(signed.payload).toBe(
      `{"v":1,"purpose":"mxbsecure-lease","steamId":"${CLEAN}","issuedAt":1700000000000,"expiresAt":${1_700_000_000_000 + LEASE_TTL_MS}}`,
    );
    expect(await verifyLease(signed, publicKey)).not.toBeNull();

    const stretched = { ...signed, payload: signed.payload.replace(/"expiresAt":\d+/, '"expiresAt":9999999999999') };
    expect(await verifyLease(stretched, publicKey)).toBeNull();
    const other = await signingPair();
    expect(await verifyLease(signed, other.publicKey)).toBeNull();
  });

  it("keeps leases and gate verdicts apart, though one key signs both", async () => {
    const { secret, publicKey, privateKey } = await signingPair();
    const leaseSigned = await signLeaseWith(CLEAN, privateKey);
    const verdictSigned = (await signVerdict({ MXB_VERDICT_SIGNING_KEY: secret } as unknown as Env, {
      status: "ok",
      account: "acc_clean",
      steamId: CLEAN,
    }))!;
    // Both signatures are genuine; each is still refused as the other kind of statement.
    expect(await verifyVerdict(leaseSigned, publicKey)).toBeNull();
    expect(await verifyLease(verdictSigned, publicKey)).toBeNull();
    expect(await verifyVerdict(verdictSigned, publicKey)).not.toBeNull();
  });
});
