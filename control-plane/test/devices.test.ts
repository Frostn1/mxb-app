/**
 * Linking installs by a keyed hash of the machine, so a ban follows the PC.
 *
 * Against the real router and a real SQLite (`d1sqlite.ts`), like `bans.test.ts`: the feature is
 * a row written at the gate and one more hop in the resolution query, and neither means anything
 * against a stub. Every machine id and GUID here is synthetic.
 */

import { describe, expect, it, vi } from "vitest";
import { APP_BLOCK_MESSAGE, addBan, banFor } from "../src/bans";
import { DEVICE_HEADER, deviceHash } from "../src/devices";
import { hashToken } from "../src/auth";
import { d1 } from "./d1sqlite";

// The entry module exports the voice Durable Object, whose base class only exists in workerd.
vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const SALT = "test-only-device-salt";
/** Synthetic GUIDs from the repository's allowlist; neither is anybody's install. */
const BANNED_GUID = "FF0110000111111111";
const CLEAN_GUID = "FF0110000122222222";
/** A synthetic machine id, and a second machine. MachineGuid-shaped, identifies nothing. */
const MACHINE = "00000000-0000-4000-8000-000000000001";
const OTHER_MACHINE = "00000000-0000-4000-8000-000000000002";

/**
 * What the app sends: SHA-256 of the domain tag and the machine id, lowercase hex. The same
 * construction as `crates/core/src/device.rs`, and the vector below is asserted on both sides.
 */
async function clientHash(machineId: string): Promise<string> {
  const bytes = new TextEncoder().encode(`mxb-device-link/v1\0${machineId.trim().toLowerCase()}`);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function deployment(overrides: Record<string, string | undefined> = {}): Env {
  return { DB: d1(), MXB_DEVICE_SALT: SALT, ...overrides } as unknown as Env;
}

const call = (env: Env, request: Request) => worker.fetch(request, env, {} as ExecutionContext);

function req(method: string, path: string, opts: { token?: string; device?: string; body?: unknown } = {}) {
  const headers: Record<string, string> = {};
  if (opts.token) headers.Authorization = `Bearer ${opts.token}`;
  if (opts.device) headers[DEVICE_HEADER] = opts.device;
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";
  return new Request(`https://cp.test${path}`, {
    method,
    headers,
    body: opts.body === undefined ? undefined : JSON.stringify(opts.body),
  });
}

async function account(env: Env, id: string, token: string, steamId: string | null, guid: string | null) {
  await env.DB.prepare(
    "INSERT INTO accounts (id, rider_name, steam_id, guid, token_hash, created_at) VALUES (?, ?, ?, ?, ?, ?)",
  )
    .bind(id, id, steamId, guid, await hashToken(token), Date.now())
    .run();
}

const gate = async (env: Env, token: string, device?: string) =>
  (await call(env, req("GET", "/v1/app/gate", { token, device }))).json() as Promise<{ status: string; message?: string }>;

/** A self-serve account minted the way the apps mint one, reporting a device. */
async function mint(env: Env, device?: string): Promise<{ accountId: string; token: string }> {
  const res = await call(env, req("POST", "/v1/account", { device, body: { riderName: "Rider" } }));
  expect(res.status).toBe(201);
  return (await res.json()) as { accountId: string; token: string };
}

async function links(env: Env): Promise<{ account_id: string; device_hash: string }[]> {
  const rows = await env.DB.prepare("SELECT account_id, device_hash FROM device_links ORDER BY account_id").all<{
    account_id: string;
    device_hash: string;
  }>();
  return rows.results ?? [];
}

describe("the client's hash", () => {
  it("matches the Rust client's, byte for byte", async () => {
    // Asserted as the same constant in `crates/core/src/device.rs`, so the two can never drift.
    expect(await clientHash(MACHINE)).toBe("e0c8264c0b6227cb35e3a45b7e8388beda6076845e3194d9ca9bbe11a03ab9d5");
  });
});

describe("recording a device", () => {
  it("links the account at the gate, stored keyed and never as reported", async () => {
    const env = deployment();
    await account(env, "acc_a", "token-a", null, CLEAN_GUID);
    const reported = await clientHash(MACHINE);

    expect((await gate(env, "token-a", reported)).status).toBe("ok");

    const rows = await links(env);
    expect(rows).toEqual([{ account_id: "acc_a", device_hash: await deviceHash(env, reported) }]);
    // Keyed with the server's secret: neither the machine id nor the client's hash is what is kept.
    expect(rows[0].device_hash).not.toBe(reported);
    expect(rows[0].device_hash).toMatch(/^[0-9a-f]{64}$/);
  });

  it("links a freshly minted account to the machine it was minted on", async () => {
    const env = deployment();
    const { accountId } = await mint(env, await clientHash(MACHINE));
    expect((await links(env)).map((r) => r.account_id)).toEqual([accountId]);
  });

  it("moves a timestamp rather than adding a row when the same install asks again", async () => {
    const env = deployment();
    await account(env, "acc_a", "token-a", null, CLEAN_GUID);
    const reported = await clientHash(MACHINE);
    await gate(env, "token-a", reported);
    await gate(env, "token-a", reported);
    expect(await links(env)).toHaveLength(1);
  });

  it("ignores a report that is not a client hash, and still answers", async () => {
    const env = deployment();
    await account(env, "acc_a", "token-a", null, CLEAN_GUID);
    // The raw machine id, a short string, upper-case junk: none of them is stored in any form.
    for (const bad of [MACHINE, "abc", "Z".repeat(64)]) {
      expect((await gate(env, "token-a", bad)).status).toBe("ok");
    }
    expect(await links(env)).toEqual([]);
  });

  it("never stores the raw machine id, in any table it could land in", async () => {
    const env = deployment();
    await account(env, "acc_a", "token-a", null, CLEAN_GUID);
    await gate(env, "token-a", await clientHash(MACHINE));
    await mint(env, await clientHash(MACHINE));

    const tables = await env.DB.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all<{ name: string }>();
    const reported = await clientHash(MACHINE);
    for (const { name } of tables.results ?? []) {
      const rows = await env.DB.prepare(`SELECT * FROM "${name}"`).all<Record<string, unknown>>();
      const dump = JSON.stringify(rows.results ?? []).toLowerCase();
      expect(dump, name).not.toContain(MACHINE);
      expect(dump, name).not.toContain(reported);
    }
  });
});

describe("a ban follows the machine", () => {
  it("resolves a fresh account on a banned PC as banned", async () => {
    const env = deployment();
    await account(env, "acc_banned", "banned-token", null, BANNED_GUID);
    const reported = await clientHash(MACHINE);
    await gate(env, "banned-token", reported);
    await addBan(env, { guid: BANNED_GUID, reason: "unlocked and shared protected content" }, "admin");

    // A brand-new token on the same PC: nothing in common with the banned account but the machine.
    const fresh = await mint(env, reported);
    expect(await gate(env, fresh.token, reported)).toEqual({ status: "unsupported", message: APP_BLOCK_MESSAGE });
    // And from then on by account id alone, which is all most endpoints have.
    expect((await banFor(env, { accountId: fresh.accountId }))?.guid).toBe(BANNED_GUID);
  });

  it("resolves a new Steam account on the banned PC too", async () => {
    const env = deployment();
    await account(env, "acc_banned", "banned-token", null, BANNED_GUID);
    const reported = await clientHash(MACHINE);
    await gate(env, "banned-token", reported);
    await addBan(env, { guid: BANNED_GUID, reason: "unlocked and shared protected content" }, "admin");

    await account(env, "acc_new_steam", "steam-token", "76561198000000099", null);
    expect((await gate(env, "steam-token", reported)).status).toBe("unsupported");
  });

  it("leaves another machine alone, and lets go when the ban is lifted", async () => {
    const env = deployment();
    await account(env, "acc_banned", "banned-token", null, BANNED_GUID);
    await gate(env, "banned-token", await clientHash(MACHINE));
    await addBan(env, { guid: BANNED_GUID, reason: "unlocked and shared protected content" }, "admin");

    const elsewhere = await mint(env, await clientHash(OTHER_MACHINE));
    expect((await gate(env, elsewhere.token, await clientHash(OTHER_MACHINE))).status).toBe("ok");

    await env.DB.prepare("UPDATE guid_bans SET lifted_at = ?, lifted_by = 'admin' WHERE guid = ?")
      .bind(Date.now(), BANNED_GUID)
      .run();
    const same = await mint(env, await clientHash(MACHINE));
    expect((await gate(env, same.token, await clientHash(MACHINE))).status).toBe("ok");
  });
});

describe("without MXB_DEVICE_SALT", () => {
  it("records nothing, resolves nothing, and never fails a request", async () => {
    const env = deployment({ MXB_DEVICE_SALT: undefined });
    await account(env, "acc_banned", "banned-token", null, BANNED_GUID);
    const reported = await clientHash(MACHINE);
    expect((await gate(env, "banned-token", reported)).status).toBe("ok");
    await addBan(env, { guid: BANNED_GUID, reason: "unlocked and shared protected content" }, "admin");

    const fresh = await mint(env, reported);
    expect((await gate(env, fresh.token, reported)).status).toBe("ok");
    expect(await links(env)).toEqual([]);
    expect(await deviceHash(env, reported)).toBeNull();
  });

  it("stops reading links already stored once the secret is taken away", async () => {
    const on = deployment();
    await account(on, "acc_banned", "banned-token", null, BANNED_GUID);
    const reported = await clientHash(MACHINE);
    await gate(on, "banned-token", reported);
    await addBan(on, { guid: BANNED_GUID, reason: "unlocked and shared protected content" }, "admin");
    const fresh = await mint(on, reported);
    expect((await gate(on, fresh.token, reported)).status).toBe("unsupported");

    const off = { ...on, MXB_DEVICE_SALT: undefined } as unknown as Env;
    expect((await gate(off, fresh.token, reported)).status).toBe("ok");
  });
});

describe("erasure", () => {
  it("deletes the account's device links, a banned account's included", async () => {
    const env = deployment();
    await account(env, "acc_banned", "banned-token", null, BANNED_GUID);
    await gate(env, "banned-token", await clientHash(MACHINE));
    await addBan(env, { guid: BANNED_GUID, reason: "unlocked and shared protected content" }, "admin");
    expect(await links(env)).toHaveLength(1);

    const res = await call(env, req("DELETE", "/v1/me", { token: "banned-token" }));
    expect(res.status).toBe(200);
    const body = (await res.json()) as { cleared: string[] };
    expect(body.cleared).toContain("device_links");
    expect(await links(env)).toEqual([]);
  });
});
