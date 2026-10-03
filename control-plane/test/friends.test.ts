import { beforeEach, describe, expect, it, vi } from "vitest";
import { hashToken } from "../src/auth";
import { FRIEND_PRESENCE_TTL_MS, MAX_OUTGOING, cleanText, formatCode, normalizeCode } from "../src/friends";
import { d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

let env: Env;

async function rider(name: string): Promise<{ id: string; token: string }> {
  const id = `acc_${name.toLowerCase()}`;
  await env.DB.prepare("INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, ?)")
    .bind(id, name, await hashToken(`tok-${name}`), Date.now())
    .run();
  return { id, token: `tok-${name}` };
}

async function call(token: string | null, method: string, path: string, body?: unknown) {
  const res = await worker.fetch(
    new Request(`https://cp.test${path}`, {
      method,
      headers: {
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(body ? { "content-type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    }),
    env,
    {} as ExecutionContext,
  );
  return { status: res.status, body: (await res.json()) as Record<string, any> };
}

/** Two accounts that are accepted friends. */
async function befriend(a: { id: string; token: string }, b: { id: string; token: string }) {
  await call(a.token, "POST", "/v1/friends/request", { accountId: b.id });
  const res = await call(b.token, "POST", "/v1/friends/respond", { accountId: a.id, accept: true });
  expect(res.body.status).toBe("friends");
}

const report = (token: string, extra: Record<string, unknown> = {}) =>
  call(token, "PUT", "/v1/friends/presence", { serverName: "Fake Test Server", address: "203.0.113.7:54210", ...extra });

beforeEach(() => {
  env = { DB: d1() } as unknown as Env;
});

describe("friend codes", () => {
  it("round-trips a code through its display form and forgives typing", () => {
    expect(formatCode("ABCDEFGH")).toBe("ABCD-EFGH");
    expect(normalizeCode("abcd-efgh")).toBe("ABCDEFGH");
    expect(normalizeCode(" ABCD EFGH ")).toBe("ABCDEFGH");
  });

  it("refuses look-alikes, wrong lengths and non-strings", () => {
    expect(normalizeCode("ABCD-EFG0")).toBeNull();
    expect(normalizeCode("ABCD-EFGI")).toBeNull();
    expect(normalizeCode("ABC")).toBeNull();
    expect(normalizeCode(12345678)).toBeNull();
  });

  it("strips control and bidi characters from free text", () => {
    expect(cleanText("  Ho\u0000st‮  name ", 64)).toBe("Host name");
    expect(cleanText("x".repeat(100), 10)).toHaveLength(10);
  });
});

describe("requests", () => {
  it("needs a session token", async () => {
    expect((await call(null, "GET", "/v1/friends")).status).toBe(401);
  });

  it("adds a friend by search, accept and list, and shows each side the other", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");

    const found = await call(alice.token, "GET", "/v1/friends/search?q=bob");
    expect(found.body.results).toEqual([{ accountId: bob.id, riderName: "Bobby", relation: "none" }]);

    expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: bob.id })).body.status).toBe("pending");
    expect((await call(bob.token, "GET", "/v1/friends")).body.incoming).toEqual([
      { accountId: alice.id, riderName: "Alice" },
    ]);
    expect((await call(alice.token, "GET", "/v1/friends/search?q=bob")).body.results[0].relation).toBe("pending_out");

    await call(bob.token, "POST", "/v1/friends/respond", { accountId: alice.id, accept: true });
    for (const [me, other] of [[alice, "Bobby"], [bob, "Alice"]] as const) {
      const list = (await call(me.token, "GET", "/v1/friends")).body;
      expect(list.friends.map((f: any) => f.riderName)).toEqual([other]);
      expect(list.incoming).toEqual([]);
    }
  });

  it("adds a friend by friend code, with or without the dash", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    const code: string = (await call(bob.token, "GET", "/v1/friends")).body.friendCode;
    expect(code).toMatch(/^[A-Z2-9]{4}-[A-Z2-9]{4}$/);

    const sent = await call(alice.token, "POST", "/v1/friends/request", { friendCode: code.replace("-", "").toLowerCase() });
    expect(sent.body).toMatchObject({ status: "pending", riderName: "Bobby" });
    expect((await call(alice.token, "POST", "/v1/friends/request", { friendCode: "ZZZZ-ZZZZ" })).status).toBe(404);
    expect((await call(alice.token, "POST", "/v1/friends/request", { friendCode: "nope" })).status).toBe(400);
  });

  it("keeps one stable friend code per account", async () => {
    const alice = await rider("Alice");
    const first = (await call(alice.token, "GET", "/v1/friends")).body.friendCode;
    expect((await call(alice.token, "GET", "/v1/friends")).body.friendCode).toBe(first);
  });

  it("treats two riders asking each other as one friendship", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await call(alice.token, "POST", "/v1/friends/request", { accountId: bob.id });
    expect((await call(bob.token, "POST", "/v1/friends/request", { accountId: alice.id })).body.status).toBe("friends");
    const rows = await env.DB.prepare("SELECT COUNT(*) AS n FROM friendships").first<{ n: number }>();
    expect(rows?.n).toBe(1);
  });

  it("refuses yourself, strangers and malformed bodies", async () => {
    const alice = await rider("Alice");
    expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: alice.id })).status).toBe(400);
    expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: "acc_nobody" })).status).toBe(404);
    expect((await call(alice.token, "POST", "/v1/friends/request", {})).status).toBe(400);
    expect((await call(alice.token, "GET", "/v1/friends/search?q=al")).status).toBe(400);
  });

  it("keeps a declined request declined, and the sender is not told", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await call(alice.token, "POST", "/v1/friends/request", { accountId: bob.id });
    await call(bob.token, "POST", "/v1/friends/respond", { accountId: alice.id, accept: false });

    expect((await call(bob.token, "GET", "/v1/friends")).body.incoming).toEqual([]);
    // Alice still reads it as pending, and asking again changes nothing.
    expect((await call(alice.token, "GET", "/v1/friends")).body.outgoing).toHaveLength(1);
    expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: bob.id })).body.status).toBe("pending");
    // Cancelling it does not clear the decline.
    await call(alice.token, "DELETE", `/v1/friends/${bob.id}`);
    expect((await call(bob.token, "GET", "/v1/friends")).body.incoming).toEqual([]);
    expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: bob.id })).body.status).toBe("pending");
  });

  it("removes a friend from both sides", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await befriend(alice, bob);
    await call(alice.token, "DELETE", `/v1/friends/${bob.id}`);
    expect((await call(alice.token, "GET", "/v1/friends")).body.friends).toEqual([]);
    expect((await call(bob.token, "GET", "/v1/friends")).body.friends).toEqual([]);
  });

  it("caps outgoing requests", async () => {
    const alice = await rider("Alice");
    for (let i = 0; i < MAX_OUTGOING; i++) {
      const other = await rider(`Other${i}`);
      expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: other.id })).status).toBe(200);
    }
    const one = await rider("OneMore");
    expect((await call(alice.token, "POST", "/v1/friends/request", { accountId: one.id })).status).toBe(429);
  });

  it("rate limits per account and bucket", async () => {
    const alice = await rider("Alice");
    const calls: string[] = [];
    (env as unknown as { FRIENDS_LIMITER: unknown }).FRIENDS_LIMITER = {
      limit: async ({ key }: { key: string }) => {
        calls.push(key);
        return { success: false };
      },
    };
    expect((await call(alice.token, "GET", "/v1/friends")).status).toBe(429);
    expect((await call(alice.token, "GET", "/v1/friends/search?q=abc")).status).toBe(429);
    expect(calls).toEqual(["acc_alice:read", "acc_alice:write"]);
  });
});

describe("presence", () => {
  it("shows a friend's server to an accepted friend only", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    const eve = await rider("Evelyn");
    await befriend(alice, bob);
    await call(eve.token, "POST", "/v1/friends/request", { accountId: bob.id }); // pending only

    expect((await report(bob.token, { track: "fake_track", riders: 3 })).body.shared).toBe(true);

    const seen = (await call(alice.token, "GET", "/v1/friends")).body.friends[0];
    expect(seen.presence).toMatchObject({
      serverName: "Fake Test Server",
      address: "203.0.113.7:54210",
      track: "fake_track",
      riders: 3,
    });
    // A pending requester sees no friends at all, let alone where Bobby is.
    expect((await call(eve.token, "GET", "/v1/friends")).body.friends).toEqual([]);
  });

  it("expires a heartbeat after the TTL", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await befriend(alice, bob);
    await report(bob.token);
    await env.DB.prepare("UPDATE friend_presence SET updated_at = ?")
      .bind(Date.now() - FRIEND_PRESENCE_TTL_MS - 1000)
      .run();
    expect((await call(alice.token, "GET", "/v1/friends")).body.friends[0].presence).toBeNull();
  });

  it("stores nothing while presence is hidden, and removes what was stored", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await befriend(alice, bob);
    await report(bob.token);
    expect((await call(alice.token, "GET", "/v1/friends")).body.friends[0].presence).not.toBeNull();

    expect((await call(bob.token, "PUT", "/v1/friends/settings", { hidePresence: true })).body.hidePresence).toBe(true);
    expect((await call(alice.token, "GET", "/v1/friends")).body.friends[0].presence).toBeNull();
    expect((await report(bob.token)).body.shared).toBe(false);
    const rows = await env.DB.prepare("SELECT COUNT(*) AS n FROM friend_presence").first<{ n: number }>();
    expect(rows?.n).toBe(0);
    expect((await call(bob.token, "GET", "/v1/friends")).body.hidePresence).toBe(true);

    await call(bob.token, "PUT", "/v1/friends/settings", { hidePresence: false });
    await report(bob.token);
    expect((await call(alice.token, "GET", "/v1/friends")).body.friends[0].presence).not.toBeNull();
  });

  it("clears on request", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await befriend(alice, bob);
    await report(bob.token);
    await call(bob.token, "DELETE", "/v1/friends/presence");
    expect((await call(alice.token, "GET", "/v1/friends")).body.friends[0].presence).toBeNull();
  });

  it("validates what a rider reports", async () => {
    const bob = await rider("Bobby");
    expect((await call(bob.token, "PUT", "/v1/friends/presence", {})).status).toBe(400);
    expect((await report(bob.token, { address: "10.0.0.1:54210" })).status).toBe(400);
    expect((await report(bob.token, { address: "-evil:1 -x" })).status).toBe(400);
    expect((await report(bob.token, { riders: 9999 })).status).toBe(400);
    expect((await report(bob.token, { riders: "3" })).status).toBe(400);
    expect((await report(bob.token, { address: undefined })).status).toBe(200);
  });

  it("is gone from every table when the account is erased", async () => {
    const alice = await rider("Alice");
    const bob = await rider("Bobby");
    await befriend(alice, bob);
    await report(bob.token);
    expect((await call(bob.token, "DELETE", "/v1/me")).status).toBe(200);
    for (const table of ["friendships", "friend_presence", "friend_profiles"]) {
      const row = await env.DB.prepare(`SELECT COUNT(*) AS n FROM ${table}`).first<{ n: number }>();
      // Alice has a profile only if she opened the panel, which she did not.
      expect(row?.n, table).toBe(0);
    }
  });
});
