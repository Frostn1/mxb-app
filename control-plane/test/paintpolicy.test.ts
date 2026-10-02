/**
 * View-only and locked paints through the real router and a real SQLite: the owner's settings,
 * who is told about a view-only paint, and the signed lock list mxbserver fetches.
 *
 * No real GUID or Steam id appears here: Steam-verified GUIDs are derived at run time from
 * synthetic Steam ids, and typed GUIDs are synthetic `FF00…` values.
 */

import { describe, expect, it, vi } from "vitest";
import { hashToken } from "../src/auth";
import { guidFromSteamId } from "../src/steam";
import { lockTarget, LOCKS_TRUSTED_COMMENT } from "../src/paintpolicy";
import { legacyFrame } from "../src/paintroom";
import { b64url } from "../src/verdict";
import { d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

const STEAM_A = "76561197960265729";
const STEAM_B = "76561197960265730";
const TEAM_GUID = "FF00000000000000A1";
const SERVER = { address: "203.0.113.9:54210" };

function bucket() {
  const objects = new Map<string, { body: ArrayBuffer; uploaded: Date }>();
  return {
    objects,
    async head(key: string) {
      const o = objects.get(key);
      return o ? { key, uploaded: o.uploaded, size: o.body.byteLength } : null;
    },
    async get(key: string) {
      const o = objects.get(key);
      return o ? { body: o.body, uploaded: o.uploaded } : null;
    },
    async put(key: string, body: ArrayBuffer) {
      objects.set(key, { body, uploaded: new Date() });
    },
  };
}

function rooms() {
  const told: { frame: unknown }[] = [];
  return {
    told,
    idFromName: (name: string) => name,
    get: () => ({
      async fetch(url: string, init: { body?: string }) {
        if (url.endsWith("/notify")) told.push({ frame: JSON.parse(init.body!) });
        return new Response(null, { status: 204 });
      },
    }),
  };
}

async function signingKey(): Promise<{ pkcs8: string; publicKey: Uint8Array }> {
  const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])) as CryptoKeyPair;
  const pkcs8 = new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer);
  const publicKey = new Uint8Array((await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer);
  return { pkcs8: b64url(pkcs8), publicKey };
}

async function deployment(mode: string | undefined = "on") {
  const DB = d1();
  const key = await signingKey();
  const accounts: [string, string, string | null][] = [
    ["acc_a", "Alice", STEAM_A],
    ["acc_b", "Bob", STEAM_B],
    ["acc_c", "Carol", null],
  ];
  for (const [id, name, steam] of accounts) {
    await DB.prepare("INSERT INTO accounts (id, rider_name, token_hash, created_at, steam_id, guid) VALUES (?, ?, ?, ?, ?, ?)")
      .bind(id, name, await hashToken(`${id}-token`), Date.now(), steam, steam ? guidFromSteamId(steam) : null)
      .run();
  }
  const now = Date.now();
  await DB.prepare(
    `INSERT INTO managed_servers
       (id, label, provider, region, lifecycle, game_endpoint, server_url, admin_token,
        deployment_revision, deployment_method, game_port, created_at, updated_at, rating_token_hash, rating_token_issued_at)
     VALUES ('srv_1', 'race host', 'ovh-vps', 'us-west', 'running', NULL, 'https://srv.example.test', 'admin-token-thats-long-enough', 'v1', 'systemd', 54210, ?, ?, ?, ?)`,
  )
    .bind(now, now, await hashToken("server-token"), now)
    .run();
  const PAINT_ROOMS = rooms();
  const env = { DB, PAINTS: bucket(), PAINT_ROOMS, MXB_PAINTLOCK_SIGNING_KEY: key.pkcs8, VIEW_ONLY_MODE: mode } as unknown as Env;
  return { env, PAINT_ROOMS, publicKey: key.publicKey };
}

const call = (env: Env, method: string, path: string, who: string, body?: unknown) =>
  worker.fetch(
    new Request(`https://cp.test${path}`, {
      method,
      headers: { Authorization: `Bearer ${who}-token`, "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
    env,
    {} as ExecutionContext,
  );

const HASH = "a".repeat(64);
const look = [
  { bikeId: "KTM450", paints: [{ slot: "paint", fileName: "Mine.pnt", sha256: HASH, size: 5, relDest: "bikes/KTM450/paints/Mine.pnt" }] },
];

interface Joined {
  riders: { riderName: string; paints: { sha256: string; viewOnly?: boolean }[] }[];
  room: string;
}

async function aliceWears(env: Env) {
  await call(env, "POST", "/v1/paintsync/join", "acc_a", { server: SERVER, bikes: look });
}

const setPolicy = (env: Env, who: string, body: unknown) => call(env, "PUT", `/v1/paints/${HASH}/policy`, who, body);

function fromB64(s: string): Uint8Array {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}

/** Check a minisign legacy signature the way `minisign-verify` does, with WebCrypto. */
async function minisignVerifies(message: Uint8Array, minisig: string, publicKey: Uint8Array): Promise<boolean> {
  const lines = minisig.split("\n");
  const sig = fromB64(lines[1]!);
  if (sig.length !== 74 || sig[0] !== 0x45 || sig[1] !== 0x64) return false;
  const keyId = new Uint8Array(await crypto.subtle.digest("SHA-256", publicKey)).slice(0, 8);
  if (!sig.slice(2, 10).every((b, i) => b === keyId[i])) return false;
  const pk = await crypto.subtle.importKey("raw", publicKey, { name: "Ed25519" }, false, ["verify"]);
  const detached = sig.slice(10);
  if (!(await crypto.subtle.verify({ name: "Ed25519" }, pk, detached, message))) return false;
  const trusted = lines[2]!.replace(/^trusted comment: /, "");
  const global = new Uint8Array([...detached, ...new TextEncoder().encode(trusted)]);
  return crypto.subtle.verify({ name: "Ed25519" }, pk, fromB64(lines[3]!), global);
}

async function locks(env: Env) {
  const res = await worker.fetch(
    new Request("https://cp.test/v1/servers/paint-locks", { headers: { Authorization: "Bearer server-token" } }),
    env,
    {} as ExecutionContext,
  );
  return res;
}

describe("paint policies", () => {
  it("lets the owner set view-only, lock and a team, and lists them back", async () => {
    const { env } = await deployment();
    await aliceWears(env);
    const res = await setPolicy(env, "acc_a", {
      viewOnly: true,
      locked: true,
      team: [{ kind: "account", id: "bob" }, { kind: "guid", id: TEAM_GUID.toLowerCase() }],
    });
    expect(res.status).toBe(200);
    const listed = (await (await call(env, "GET", "/v1/paints/policies", "acc_a")).json()) as {
      canLock: boolean;
      paints: { sha256: string; viewOnly: boolean; locked: boolean; team: { kind: string; id: string }[] }[];
    };
    expect(listed.canLock).toBe(true);
    expect(listed.paints).toHaveLength(1);
    expect(listed.paints[0]).toMatchObject({ sha256: HASH, viewOnly: true, locked: true });
    expect(listed.paints[0]!.team).toEqual([
      { kind: "account", id: "Bob" },
      { kind: "guid", id: TEAM_GUID },
    ]);
  });

  it("refuses a paint the caller doesn't wear, and a lock without Steam", async () => {
    const { env } = await deployment();
    await aliceWears(env);
    expect((await setPolicy(env, "acc_b", { viewOnly: true, locked: false })).status).toBe(404);
    await call(env, "POST", "/v1/paintsync/join", "acc_c", { server: SERVER, bikes: look });
    const res = await setPolicy(env, "acc_c", { viewOnly: false, locked: true });
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({ code: "steam_required" });
    // View-only alone needs no Steam: it is about keeping, not identity.
    expect((await setPolicy(env, "acc_c", { viewOnly: true, locked: false })).status).toBe(200);
  });

  it("saving everything off removes the policy", async () => {
    const { env } = await deployment();
    await aliceWears(env);
    await setPolicy(env, "acc_a", { viewOnly: true, locked: true, team: [{ kind: "guid", id: TEAM_GUID }] });
    await setPolicy(env, "acc_a", { viewOnly: false, locked: false, team: [] });
    const rows = await env.DB.prepare("SELECT COUNT(*) AS n FROM paint_policies").first<{ n: number }>();
    const shares = await env.DB.prepare("SELECT COUNT(*) AS n FROM paint_shares").first<{ n: number }>();
    expect(rows?.n).toBe(0);
    expect(shares?.n).toBe(0);
  });
});

describe("view-only paints reach only apps that honour them", () => {
  it("flags them for a new app and leaves them out for an old one and the roster", async () => {
    const { env, PAINT_ROOMS } = await deployment();
    await aliceWears(env);
    await setPolicy(env, "acc_a", { viewOnly: true, locked: false });

    const fresh = (await (await call(env, "POST", "/v1/paintsync/join", "acc_b", { server: SERVER, caps: ["viewOnly"] })).json()) as Joined;
    expect(fresh.riders[0]!.paints).toEqual([expect.objectContaining({ sha256: HASH, viewOnly: true })]);
    expect(fresh.room).toContain("caps=viewOnly");

    const old = (await (await call(env, "POST", "/v1/paintsync/join", "acc_c", { server: SERVER })).json()) as Joined;
    expect(old.riders.find((r) => r.riderName === "Alice")!.paints).toEqual([]);
    expect(old.room).not.toContain("caps");

    const roster = (await (await call(env, "GET", "/v1/roster?server=addr:203.0.113.9:54210", "acc_c")).json()) as {
      riders: { riderName: string; paints: unknown[] }[];
    };
    expect(roster.riders.find((r) => r.riderName === "Alice")).toBeUndefined();

    // Alice re-joining tells the room with the flag on; the room strips it for old sockets.
    await aliceWears(env);
    const frame = JSON.stringify(PAINT_ROOMS.told.at(-1)!.frame);
    expect(frame).toContain('"viewOnly":true');
    expect(JSON.parse(legacyFrame(frame)).rider.paints).toEqual([]);
    expect(legacyFrame('{"t":"left","riderName":"Alice"}')).toBe('{"t":"left","riderName":"Alice"}');
  });
});

describe("VIEW_ONLY_MODE off (the default) pulls view-only and locked paints", () => {
  for (const mode of [undefined, "off"]) {
    it(`refuses the policy and delivers nothing restricted (mode ${mode})`, async () => {
      const { env } = await deployment("on");
      await aliceWears(env);
      // Set while on, as if left over from before the pull.
      expect((await setPolicy(env, "acc_a", { viewOnly: true, locked: true })).status).toBe(200);
      (env as unknown as { VIEW_ONLY_MODE?: string }).VIEW_ONLY_MODE = mode;

      const refused = await setPolicy(env, "acc_a", { viewOnly: true, locked: false });
      expect(refused.status).toBe(409);
      expect(await refused.json()).toEqual({ error: "view_only_unavailable" });
      expect((await setPolicy(env, "acc_a", { viewOnly: false, locked: true })).status).toBe(409);

      // Whatever capability the app sends, the restricted paint is not delivered.
      for (const caps of [["viewOnly"], undefined]) {
        const j = (await (await call(env, "POST", "/v1/paintsync/join", "acc_b", { server: SERVER, caps })).json()) as Joined;
        expect(j.riders.find((r) => r.riderName === "Alice")!.paints).toEqual([]);
      }

      // Existing rows untouched, and the app is told it is unavailable.
      const list = (await (await call(env, "GET", "/v1/paints/policies", "acc_a")).json()) as {
        viewOnlyAvailable: boolean;
        paints: { viewOnly: boolean; locked: boolean }[];
      };
      expect(list.viewOnlyAvailable).toBe(false);
      expect(list.paints[0]).toMatchObject({ viewOnly: true, locked: true });

      // Turning it back on restores them.
      (env as unknown as { VIEW_ONLY_MODE?: string }).VIEW_ONLY_MODE = "on";
      const back = (await (await call(env, "POST", "/v1/paintsync/join", "acc_c", { server: SERVER, caps: ["viewOnly"] })).json()) as Joined;
      expect(back.riders.find((r) => r.riderName === "Alice")!.paints).toEqual([expect.objectContaining({ viewOnly: true })]);
    });
  }

  it("still allows clearing a policy, and a locked-only paint is also withheld from the roster", async () => {
    const { env } = await deployment("on");
    await aliceWears(env);
    await setPolicy(env, "acc_a", { viewOnly: false, locked: true });
    (env as unknown as { VIEW_ONLY_MODE?: string }).VIEW_ONLY_MODE = "off";
    const roster = (await (await call(env, "GET", "/v1/roster?server=addr:203.0.113.9:54210", "acc_c")).json()) as {
      riders: { riderName: string }[];
    };
    expect(roster.riders.find((r) => r.riderName === "Alice")).toBeUndefined();
    expect((await setPolicy(env, "acc_a", { viewOnly: false, locked: false })).status).toBe(200);
  });
});

describe("the signed lock list", () => {
  it("names the owner and the team, signed so the server can check it", async () => {
    const { env, publicKey } = await deployment();
    await aliceWears(env);
    await setPolicy(env, "acc_a", {
      viewOnly: false,
      locked: true,
      team: [{ kind: "account", id: "Bob" }, { kind: "account", id: "Carol" }, { kind: "guid", id: TEAM_GUID }],
    });
    const res = await locks(env);
    expect(res.status).toBe(200);
    const { payload, minisig } = (await res.json()) as { payload: string; minisig: string };
    expect(minisig).toContain(`trusted comment: ${LOCKS_TRUSTED_COMMENT}`);
    expect(await minisignVerifies(new TextEncoder().encode(payload), minisig, publicKey)).toBe(true);
    expect(await minisignVerifies(new TextEncoder().encode(payload + " "), minisig, publicKey)).toBe(false);

    const doc = JSON.parse(payload) as { v: number; server: string; locks: { sha256: string; bike: string; paint: string; allowed: string[] }[] };
    expect(doc.v).toBe(1);
    expect(doc.server).toBe("srv_1");
    // Carol has no Steam sign-in, so her account adds nothing; a typed GUID counts as typed.
    expect(doc.locks).toEqual([
      { sha256: HASH, bike: "ktm450", paint: "mine", allowed: [guidFromSteamId(STEAM_A), guidFromSteamId(STEAM_B), TEAM_GUID] },
    ]);
  });

  it("needs the server's token, and serves the public key in minisign format", async () => {
    const { env, publicKey } = await deployment();
    const anon = await worker.fetch(new Request("https://cp.test/v1/servers/paint-locks"), env, {} as ExecutionContext);
    expect(anon.status).toBe(401);
    const pub = await (await worker.fetch(new Request("https://cp.test/v1/paint-locks/pubkey"), env, {} as ExecutionContext)).text();
    const bin = fromB64(pub.split("\n")[1]!);
    expect(bin.length).toBe(42);
    expect([...bin.slice(10)]).toEqual([...publicKey]);
    const unsigned = { ...env, MXB_PAINTLOCK_SIGNING_KEY: undefined } as Env;
    expect((await locks(unsigned)).status).toBe(503);
  });

  it("matches the identity's bike folder and paint name", () => {
    expect(lockTarget("bikes/KTM450/paints/Mine.pnt", "KTM450")).toEqual({ bike: "ktm450", paint: "mine" });
    expect(lockTarget("paints/bikes/Mine.pnt", "Yam250")).toEqual({ bike: "yam250", paint: "mine" });
    expect(lockTarget("rider/helmets/H1/paints/Mine.pnt", "KTM450")).toBeNull();
  });
});

