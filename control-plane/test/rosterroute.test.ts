/**
 * `GET /v1/roster` is two endpoints on one path: the public server book, and the bearer paint
 * roster behind `?server=`. The book used to answer both, so every paint sync got
 * `{ addresses }` and the app failed with "missing field `riders`".
 */

import { describe, expect, it, vi } from "vitest";
import { hashToken } from "../src/auth";
import { d1 } from "./d1sqlite";

vi.mock("cloudflare:workers", () => ({ DurableObject: class {} }));
const { default: worker } = await import("../src/index");

async function deployment(): Promise<Env> {
  const DB = d1();
  await DB.prepare("INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, ?)")
    .bind("acc_rider", "Rider", await hashToken("rider-token"), Date.now())
    .run();
  return { DB } as unknown as Env;
}

const get = (env: Env, path: string, token?: string) =>
  worker.fetch(
    new Request(`https://cp.test${path}`, { headers: token ? { Authorization: `Bearer ${token}` } : {} }),
    env,
    {} as ExecutionContext,
  );

describe("GET /v1/roster", () => {
  it("serves the paint roster, with riders, when a server is named", async () => {
    const env = await deployment();
    const res = await get(env, "/v1/roster?server=srv_1&here=1", "rider-token");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { riders: { riderName: string }[] };
    expect(Array.isArray(body.riders)).toBe(true);
    expect(body).not.toHaveProperty("addresses");
  });

  it("still serves the public server book when no server is named", async () => {
    const env = await deployment();
    const res = await get(env, "/v1/roster");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { addresses: string[] };
    expect(Array.isArray(body.addresses)).toBe(true);
  });

  it("does not hand the paint roster to someone without an account", async () => {
    const env = await deployment();
    const res = await get(env, "/v1/roster?server=srv_1");
    expect(res.status).toBe(401);
  });
});
