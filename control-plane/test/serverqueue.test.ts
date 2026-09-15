import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  LAUNCH_GRACE_MS,
  QUEUE_TTL_MS,
  leaveQueue,
  pruneQueue,
  putQueue,
  queueCounts,
} from "../src/serverqueue";
import { d1 } from "./d1sqlite";

const SERVER = "203.0.113.7:54210";
const OTHER = "198.51.100.2:54210";

let e: Env;
let clock = 1_000_000;

async function account(id: string): Promise<string> {
  await e.DB.prepare(
    "INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, 0)",
  )
    .bind(id, `rider-${id}`, `hash-${id}`)
    .run();
  return id;
}

async function beat(id: string, server = SERVER, launched = false) {
  const res = await putQueue(
    new Request("https://cp.invalid/v1/queue", {
      method: "PUT",
      body: JSON.stringify({ server, launched }),
    }),
    id,
    e,
  );
  expect(res.status).toBe(200);
  return (await res.json()) as { position: number; ahead: number; waiting: number };
}

async function counts(...servers: string[]) {
  const url = new URL(`https://cp.invalid/v1/queue/counts?server=${servers.join(",")}`);
  return ((await (await queueCounts(url, e)).json()) as { servers: Record<string, number> })
    .servers;
}

function advance(ms: number) {
  clock += ms;
}

beforeEach(async () => {
  e = { DB: d1() } as unknown as Env;
  vi.spyOn(Date, "now").mockImplementation(() => clock);
  for (const id of ["a", "b", "c"]) await account(id);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("server queue", () => {
  it("orders riders by when they joined and keeps the place across heartbeats", async () => {
    expect(await beat("a")).toEqual({ position: 1, ahead: 0, waiting: 1 });
    advance(1000);
    expect(await beat("b")).toEqual({ position: 2, ahead: 1, waiting: 2 });
    advance(1000);
    expect(await beat("c")).toEqual({ position: 3, ahead: 2, waiting: 3 });
    advance(10_000);
    // A heartbeat doesn't move anyone.
    expect((await beat("a")).position).toBe(1);
    expect((await beat("c")).position).toBe(3);
  });

  it("breaks a tie on joined_at the same way for everyone", async () => {
    await beat("a");
    await beat("b");
    const a = await beat("a");
    const b = await beat("b");
    expect([a.ahead, b.ahead].sort()).toEqual([0, 1]);
  });

  it("drops a rider whose app went quiet, and sends them to the back on return", async () => {
    await beat("a");
    advance(1000);
    await beat("b");
    advance(QUEUE_TTL_MS + 1);
    expect(await beat("b")).toEqual({ position: 1, ahead: 0, waiting: 1 });
    advance(1000);
    expect((await beat("a")).position).toBe(2);
  });

  it("counts a launched rider as ahead until the grace runs out", async () => {
    await beat("a");
    advance(1000);
    await beat("b");
    await beat("a", SERVER, true);
    // b still waits behind a, but a no longer counts as waiting.
    expect(await beat("b")).toEqual({ position: 2, ahead: 1, waiting: 1 });
    // Both apps keep beating every 10 s, the way they do for real.
    for (let t = 0; t <= LAUNCH_GRACE_MS; t += 10_000) {
      advance(10_000);
      await beat("a", SERVER, true);
      await beat("b");
    }
    expect(await beat("b")).toEqual({ position: 1, ahead: 0, waiting: 1 });
  });

  it("moves a rider who switches server to the back of the new line", async () => {
    await beat("b", OTHER);
    advance(1000);
    await beat("a");
    advance(1000);
    await beat("a", OTHER);
    expect((await beat("a", OTHER)).position).toBe(2);
    expect(await counts(SERVER, OTHER)).toEqual({ [OTHER]: 2 });
  });

  it("leaves the line", async () => {
    await beat("a");
    advance(1000);
    await beat("b");
    await leaveQueue("a", e);
    expect((await beat("b")).position).toBe(1);
  });

  it("counts only waiting, fresh riders", async () => {
    await beat("a");
    await beat("b");
    await beat("c", OTHER);
    await beat("b", SERVER, true);
    expect(await counts(SERVER, OTHER)).toEqual({ [SERVER]: 1, [OTHER]: 1 });
    advance(QUEUE_TTL_MS + 1);
    expect(await counts(SERVER, OTHER)).toEqual({});
  });

  it("rejects a bad server key", async () => {
    const res = await putQueue(
      new Request("https://cp.invalid/v1/queue", { method: "PUT", body: "{}" }),
      "a",
      e,
    );
    expect(res.status).toBe(400);
  });

  it("prunes rows an hour old", async () => {
    await beat("a");
    advance(60 * 60 * 1000 + 1);
    await pruneQueue(e);
    const row = await e.DB.prepare("SELECT COUNT(*) AS n FROM server_queue").first<{ n: number }>();
    expect(row?.n).toBe(0);
  });
});
