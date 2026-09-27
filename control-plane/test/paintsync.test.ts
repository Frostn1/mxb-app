/**
 * Paint sync v2 through the real router and a real SQLite: join, delta upload, the room
 * notification, leave, and the day-long expiry.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { hashToken } from "../src/auth";
import { LIVE_REFRESH_MS, LIVE_TTL_MS, foldName, normalizeAddress, parseServerHint, pruneLivePaints } from "../src/paintsync";
import { d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

/** Enough R2 for paint sync: objects with an upload time the test can move. */
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
    async delete(keys: string | string[]) {
      for (const k of Array.isArray(keys) ? keys : [keys]) objects.delete(k);
    },
    async list({ prefix }: { prefix: string }) {
      return {
        objects: [...objects.entries()]
          .filter(([k]) => k.startsWith(prefix))
          .map(([key, o]) => ({ key, uploaded: o.uploaded })),
        truncated: false,
      };
    },
  };
}

/** A room that records what it was told to relay. */
function rooms() {
  const told: { server: string; except: string; frame: unknown }[] = [];
  return {
    told,
    idFromName: (name: string) => name,
    get: (name: string) => ({
      async fetch(url: string, init: { headers: Record<string, string>; body?: string }) {
        if (url.endsWith("/notify")) {
          told.push({ server: name, except: init.headers["X-Account-Id"]!, frame: JSON.parse(init.body!) });
        }
        return new Response(null, { status: 204 });
      },
    }),
  };
}

async function deployment() {
  const DB = d1();
  const PAINTS = bucket();
  const PAINT_ROOMS = rooms();
  for (const [id, name] of [["acc_a", "Alice"], ["acc_b", "Bob"]] as const) {
    await DB.prepare("INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, ?)")
      .bind(id, name, await hashToken(`${id}-token`), Date.now())
      .run();
  }
  return { env: { DB, PAINTS, PAINT_ROOMS } as unknown as Env, PAINTS, PAINT_ROOMS };
}

async function sha(bytes: Uint8Array): Promise<string> {
  const d = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

const call = (env: Env, method: string, path: string, who: string, body?: unknown) =>
  worker.fetch(
    new Request(`https://cp.test${path}`, {
      method,
      headers: { Authorization: `Bearer ${who}-token`, "Content-Type": "application/json" },
      body: body === undefined ? undefined : body instanceof Uint8Array ? body : JSON.stringify(body),
    }),
    env,
    {} as ExecutionContext,
  );

const look = (fileName: string, hash: string) => [
  { bikeId: "KTM450", paints: [{ slot: "paint", fileName, sha256: hash, size: 5, relDest: `bikes/KTM450/paints/${fileName}` }] },
];

interface JoinAnswer {
  server: string;
  missing: string[];
  riders: { riderName: string; paints: { sha256: string }[]; joinedAt: number }[];
  room: string;
}

afterEach(() => vi.useRealTimers());

describe("paint sync v2", () => {
  it("joins, uploads only what is missing, and shows each rider the other", async () => {
    const { env, PAINTS, PAINT_ROOMS } = await deployment();
    const red = new TextEncoder().encode("red!!");
    const redSha = await sha(red);

    const first = await call(env, "POST", "/v1/paintsync/join", "acc_a", {
      server: { address: "203.0.113.5:54210", name: "Frost EU" },
      bikes: look("Red.pnt", redSha),
    });
    expect(first.status).toBe(200);
    const a = (await first.json()) as JoinAnswer;
    expect(a.server).toBe("addr:203.0.113.5:54210");
    expect(a.missing).toEqual([redSha]);
    expect(a.riders).toEqual([]);

    expect((await call(env, "PUT", `/v1/paintsync/paints/${redSha}`, "acc_a", red)).status).toBe(201);
    expect(PAINTS.objects.has(`live/${redSha}`)).toBe(true);

    // Joining again with the same look is a heartbeat: nothing left to upload.
    const again = (await (await call(env, "POST", "/v1/paintsync/join", "acc_a", {
      server: { address: "203.0.113.5:54210" },
      bikes: look("Red.pnt", redSha),
    })).json()) as JoinAnswer;
    expect(again.missing).toEqual([]);

    // Bob picked the server in the game's browser, so the app only knows its name. The name
    // resolves to the address Alice's app saw it under: the same room.
    const b = (await (await call(env, "POST", "/v1/paintsync/join", "acc_b", {
      server: { name: "  frost   eu " },
      bikes: look("Red.pnt", redSha),
    })).json()) as JoinAnswer;
    expect(b.server).toBe("addr:203.0.113.5:54210");
    expect(b.missing).toEqual([]);
    expect(b.riders.map((r) => r.riderName)).toEqual(["Alice"]);
    expect(b.riders[0]!.paints[0]!.sha256).toBe(redSha);

    // Alice's room was told Bob arrived, and not Bob himself.
    const toldAboutBob = PAINT_ROOMS.told.filter((t) => t.except === "acc_b");
    expect(toldAboutBob.at(-1)).toMatchObject({ server: b.server, frame: { t: "joined", rider: { riderName: "Bob" } } });

    // The shared download route serves the day-long copy.
    const got = await call(env, "GET", `/v1/paints/${redSha}`, "acc_b");
    expect(got.status).toBe(200);
    expect(new Uint8Array(await got.arrayBuffer())).toEqual(red);

    // Leaving tells the room and takes Bob off the grid.
    expect((await call(env, "POST", "/v1/paintsync/leave", "acc_b", { server: b.server })).status).toBe(200);
    expect(PAINT_ROOMS.told.at(-1)).toMatchObject({ frame: { t: "left", riderName: "Bob" } });
    const after = (await (await call(env, "POST", "/v1/paintsync/join", "acc_a", {
      server: { address: "203.0.113.5:54210" },
    })).json()) as JoinAnswer;
    expect(after.riders).toEqual([]);
  });

  it("refuses to store a paint that isn't in the uploader's look, or doesn't match its hash", async () => {
    const { env } = await deployment();
    const bytes = new TextEncoder().encode("hello");
    const hash = await sha(bytes);
    expect((await call(env, "PUT", `/v1/paintsync/paints/${hash}`, "acc_a", bytes)).status).toBe(403);

    await call(env, "POST", "/v1/paintsync/join", "acc_a", { server: { address: "1.2.3.4:1" }, bikes: look("A.pnt", hash) });
    expect((await call(env, "PUT", `/v1/paintsync/paints/${hash}`, "acc_a", new TextEncoder().encode("other"))).status).toBe(400);
  });

  it("asks for a re-upload before a stored paint expires, and sweeps it after a day", async () => {
    const { env, PAINTS } = await deployment();
    const bytes = new TextEncoder().encode("blue!");
    const hash = await sha(bytes);
    const join = async () =>
      (await (await call(env, "POST", "/v1/paintsync/join", "acc_a", {
        server: { address: "1.2.3.4:54210" },
        bikes: look("Blue.pnt", hash),
      })).json()) as JoinAnswer;

    await join();
    await call(env, "PUT", `/v1/paintsync/paints/${hash}`, "acc_a", bytes);
    PAINTS.objects.get(`live/${hash}`)!.uploaded = new Date(Date.now() - LIVE_REFRESH_MS - 1000);
    expect((await join()).missing).toEqual([hash]);

    PAINTS.objects.get(`live/${hash}`)!.uploaded = new Date(Date.now() - LIVE_TTL_MS - 1000);
    await pruneLivePaints(env);
    expect(PAINTS.objects.has(`live/${hash}`)).toBe(false);
    // The rows go with it, so an older app's roster never names a hash that 404s.
    const rows = await env.DB.prepare("SELECT COUNT(*) AS n FROM loadout_paints WHERE sha256 = ?").bind(hash).first<{ n: number }>();
    expect(rows?.n).toBe(0);
  });

  it("only takes a server it can name", () => {
    expect(normalizeAddress("10.0.0.5:54210")).toBe("10.0.0.5:54210");
    expect(normalizeAddress("10.0.0.5")).toBeNull();
    expect(normalizeAddress("10.0.0.5:70000")).toBeNull();
    expect(parseServerHint({})).toBe("a server address or name is required");
    expect(parseServerHint({ address: "nope" })).toBe("that isn't a server address");
    expect(foldName("  Frost's  Test #2 ")).toBe("frost's test #2");
  });
});
