import { describe, expect, it } from "vitest";
import {
  claimDeviceAccount,
  MAX_DEVICE_CLAIMS_PER_DAY,
  overSignalBudget,
  parseClientMessage,
  peerViews,
  SIGNAL_BURST,
  SIGNAL_WINDOW_MS,
  voiceRoom,
  type PeerIdentity,
} from "../src/voice";

function identity(over: Partial<PeerIdentity> = {}): PeerIdentity {
  return {
    peerId: "p1",
    accountId: "a1",
    riderName: "Frost",
    raceNum: 7,
    ready: true,
    signalCount: 0,
    signalSince: 0,
    ...over,
  };
}

describe("parseClientMessage", () => {
  it("takes a hello with a rider and a race number", () => {
    expect(parseClientMessage(JSON.stringify({ t: "hello", riderName: " Frost ", raceNum: 42 }))).toEqual({
      t: "hello",
      riderName: "Frost",
      raceNum: 42,
    });
  });

  it("allows a hello before the game has said who is on the grid", () => {
    // The app connects as soon as it knows the server, which is earlier than the entry list.
    expect(parseClientMessage(JSON.stringify({ t: "hello" }))).toEqual({
      t: "hello",
      riderName: "",
      raceNum: 0,
    });
  });

  it("relays the three ICE message kinds and nothing else", () => {
    for (const kind of ["offer", "answer", "candidate"]) {
      expect(parseClientMessage(JSON.stringify({ t: "signal", to: "p2", kind, data: "x" }))).toEqual({
        t: "signal",
        to: "p2",
        kind,
        data: "x",
      });
    }
    expect(parseClientMessage(JSON.stringify({ t: "signal", to: "p2", kind: "evil", data: "x" }))).toBe(
      "unknown signal kind",
    );
  });

  it("rejects what a client should never send", () => {
    expect(parseClientMessage(null)).toBe("expected text frames");
    expect(parseClientMessage("{")).toBe("not JSON");
    expect(parseClientMessage("[]")).toBe("unknown message type");
    expect(parseClientMessage('"a string"')).toBe("not an object");
    expect(parseClientMessage(JSON.stringify({ t: "nope" }))).toBe("unknown message type");
    expect(parseClientMessage(JSON.stringify({ t: "signal", kind: "offer", data: "x" }))).toBe(
      "signal needs a peer to send to",
    );
    expect(parseClientMessage(JSON.stringify({ t: "signal", to: "p2", kind: "offer" }))).toBe(
      "signal needs data",
    );
  });

  it("refuses a rider name that could corrupt a display", () => {
    expect(parseClientMessage(JSON.stringify({ t: "hello", riderName: "a\u0001b" }))).toBe(
      "riderName has control characters",
    );
    expect(parseClientMessage(JSON.stringify({ t: "hello", riderName: "x".repeat(65) }))).toBe(
      "riderName too long",
    );
  });

  it("refuses a race number the game could never produce", () => {
    expect(parseClientMessage(JSON.stringify({ t: "hello", raceNum: -1 }))).toBe("raceNum out of range");
    expect(parseClientMessage(JSON.stringify({ t: "hello", raceNum: 10_000 }))).toBe("raceNum out of range");
    // Truncated rather than rejected: a float here is a client bug, not an attack.
    expect(parseClientMessage(JSON.stringify({ t: "hello", raceNum: 7.9 }))).toMatchObject({ raceNum: 7 });
  });

  it("refuses a frame too large to be ICE", () => {
    expect(parseClientMessage("x".repeat(17_000))).toBe("message too large");
  });
});

describe("peerViews", () => {
  it("hands a newcomer everyone but itself", () => {
    const peers = [identity({ peerId: "p1" }), identity({ peerId: "p2", accountId: "a2" })];
    expect(peerViews(peers, "p1")).toEqual([{ peerId: "p2", riderName: "Frost", raceNum: 7 }]);
  });

  it("leaves out a socket that hasn't introduced itself", () => {
    const peers = [identity({ peerId: "p2", ready: false })];
    expect(peerViews(peers, "p1")).toEqual([]);
  });

  it("carries only the claims a receiver checks for itself", () => {
    // Never the account id: peers have no business learning each other's account.
    const [view] = peerViews([identity({ peerId: "p2" })], "p1");
    expect(Object.keys(view).sort()).toEqual(["peerId", "raceNum", "riderName"]);
  });
});

describe("overSignalBudget", () => {
  it("allows a join burst and then stops", () => {
    const peer = identity({ signalSince: 1000 });
    for (let i = 0; i < SIGNAL_BURST; i++) {
      expect(overSignalBudget(peer, 1000)).toBe(false);
    }
    expect(overSignalBudget(peer, 1000)).toBe(true);
  });

  it("forgives once the window has passed", () => {
    const peer = identity({ signalSince: 1000, signalCount: SIGNAL_BURST });
    expect(overSignalBudget(peer, 1000 + SIGNAL_WINDOW_MS + 1)).toBe(false);
  });
});

// ---------------------------------------------------------------------------------------
// Admission
// ---------------------------------------------------------------------------------------

/** A D1 stand-in: one presence row, and a record of what was written. */
function stubDb(presence: { server_id: string; updated_at: number } | null) {
  const writes: string[] = [];
  return {
    writes,
    prepare(sql: string) {
      return {
        bind(...args: unknown[]) {
          return {
            async first() {
              if (sql.includes("FROM presence")) {
                return presence && args[1] === presence.server_id
                  ? { updated_at: presence.updated_at }
                  : null;
              }
              if (sql.includes("FROM device_claims")) return null;
              return null;
            },
            async run() {
              writes.push(sql);
            },
          };
        },
      };
    },
    async batch(statements: unknown[]) {
      writes.push(`batch:${statements.length}`);
      return [];
    },
  };
}

/** A Durable Object namespace that records what it was asked for. */
function stubRooms() {
  const named: string[] = [];
  return {
    named,
    idFromName(name: string) {
      named.push(name);
      return { name };
    },
    get() {
      return {
        async fetch(_url: string, init: RequestInit) {
          // 200, not the 101 the real object answers with: `new Response` refuses to
          // construct a 101 outside the Workers runtime.
          return new Response(JSON.stringify({ joined: true, headers: init.headers }), { status: 200 });
        },
      };
    },
  };
}

function roomRequest(): Request {
  return new Request("https://cp.test/v1/voice/room?server=203.0.113.10:54210", {
    headers: { Upgrade: "websocket" },
  });
}

describe("voiceRoom", () => {
  it("names the room after the server, so the first rider there creates it", async () => {
    const rooms = stubRooms();
    const env = { DB: stubDb({ server_id: "203.0.113.10:54210", updated_at: Date.now() }), VOICE_ROOMS: rooms };
    const url = new URL(roomRequest().url);

    const resp = await voiceRoom(roomRequest(), url, { id: "a1" }, env as unknown as Env);

    expect(resp.status).toBe(200);
    expect(rooms.named).toEqual(["203.0.113.10:54210"]);
  });

  it("turns away a rider who never said they were on that server", async () => {
    const env = { DB: stubDb(null), VOICE_ROOMS: stubRooms() };
    const resp = await voiceRoom(roomRequest(), new URL(roomRequest().url), { id: "a1" }, env as unknown as Env);
    expect(resp.status).toBe(403);
  });

  it("turns away a rider whose presence has gone stale", async () => {
    // Left the server an hour ago; the room should not still be theirs to join.
    const stale = { server_id: "203.0.113.10:54210", updated_at: Date.now() - 60 * 60 * 1000 };
    const env = { DB: stubDb(stale), VOICE_ROOMS: stubRooms() };
    const resp = await voiceRoom(roomRequest(), new URL(roomRequest().url), { id: "a1" }, env as unknown as Env);
    expect(resp.status).toBe(403);
  });

  it("refuses a plain GET — this endpoint is only ever an upgrade", async () => {
    const plain = new Request("https://cp.test/v1/voice/room?server=x");
    const env = { DB: stubDb(null), VOICE_ROOMS: stubRooms() };
    const resp = await voiceRoom(plain, new URL(plain.url), { id: "a1" }, env as unknown as Env);
    expect(resp.status).toBe(426);
  });

  it("proves who the caller is with a header the caller cannot set", async () => {
    const rooms = stubRooms();
    const env = { DB: stubDb({ server_id: "203.0.113.10:54210", updated_at: Date.now() }), VOICE_ROOMS: rooms };
    // A caller trying to be someone else — the forwarded request is built fresh, so their
    // header never reaches the room.
    const spoofed = new Request("https://cp.test/v1/voice/room?server=203.0.113.10:54210", {
      headers: { Upgrade: "websocket", "X-Account-Id": "somebody-else" },
    });

    const resp = await voiceRoom(spoofed, new URL(spoofed.url), { id: "a1" }, env as unknown as Env);
    const body = (await resp.json()) as { headers: Record<string, string> };
    expect(body.headers["X-Account-Id"]).toBe("a1");
  });
});

describe("claimDeviceAccount", () => {
  function signupRequest(riderName: unknown = "Frost"): Request {
    return new Request("https://cp.test/v1/account", {
      method: "POST",
      headers: { "CF-Connecting-IP": "203.0.113.5" },
      body: JSON.stringify({ riderName }),
    });
  }

  it("mints an account with no invite, and says it is a device one", async () => {
    const db = stubDb(null);
    const resp = await claimDeviceAccount(signupRequest(), { DB: db } as unknown as Env);
    expect(resp.status).toBe(201);
    const body = (await resp.json()) as { token: string; kind: string };
    expect(body.kind).toBe("device");
    expect(body.token.length).toBeGreaterThan(20);
  });

  it("needs a rider name", async () => {
    const resp = await claimDeviceAccount(signupRequest(""), { DB: stubDb(null) } as unknown as Env);
    expect(resp.status).toBe(400);
  });

  it("stops one address minting accounts all day", async () => {
    const db = {
      ...stubDb(null),
      prepare(sql: string) {
        return {
          bind() {
            return {
              async first() {
                return sql.includes("device_claims") ? { claims: MAX_DEVICE_CLAIMS_PER_DAY } : null;
              },
              async run() {},
            };
          },
        };
      },
    };
    const resp = await claimDeviceAccount(signupRequest(), { DB: db } as unknown as Env);
    expect(resp.status).toBe(429);
  });

  it("never stores the address itself", async () => {
    const bound: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        return {
          bind(...args: unknown[]) {
            bound.push([sql, ...args]);
            return { async first() { return null; }, async run() {} };
          },
        };
      },
      async batch() {
        return [];
      },
    };
    await claimDeviceAccount(signupRequest(), { DB: db } as unknown as Env);
    const flat = JSON.stringify(bound);
    expect(flat).not.toContain("203.0.113.5");
  });
});
