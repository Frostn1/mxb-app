import { describe, expect, it } from "vitest";
import { d1 } from "./d1sqlite";
import { hashToken } from "../src/auth";
import { publishShare, readShare } from "../src/liveshare";
import { grantSupporter, requestIsSupporter, supporterOwnerSql } from "../src/supporters";

const STEAM = "76561198000000001";
const TOKEN = "tok-supporter";
const PLAIN = "tok-plain";

async function env(): Promise<Env> {
  const DB = d1();
  const insert = "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)";
  await DB.prepare(insert).bind("acc-1", "Fan", STEAM, await hashToken(TOKEN), 1).run();
  await DB.prepare(insert).bind("acc-2", "Other", null, await hashToken(PLAIN), 1).run();
  const e = { DB } as unknown as Env;
  await grantSupporter(e, STEAM, "manual");
  return e;
}

function share(token?: string): Request {
  return new Request("https://cp.invalid/v1/share", {
    method: "POST",
    headers: token ? { Authorization: `Bearer ${token}` } : {},
    body: JSON.stringify({
      name: "RedBud",
      manifest: {
        items: [{ name: "RedBud.pkz", rel: "tracks/EU/RedBud.pkz", size: 10, isDir: false }],
        totalSize: 10,
        bundle: { url: "https://files.catbox.moe/abc123.zip", host: "catbox", size: 10, parts: [] },
      },
    }),
  });
}

async function badgeOf(e: Env, token?: string): Promise<unknown> {
  const made = (await (await publishShare(share(token), e)).json()) as { code: string };
  const read = await readShare(new Request("https://cp.invalid/"), made.code, e);
  return ((await read.json()) as { supporter: unknown }).supporter;
}

describe("supporter badge", () => {
  it("knows a supporter's bearer, and nobody else's", async () => {
    const e = await env();
    const req = (t?: string) =>
      new Request("https://cp.invalid/", { headers: t ? { Authorization: `Bearer ${t}` } : {} });
    expect(await requestIsSupporter(req(TOKEN), e)).toBe(true);
    expect(await requestIsSupporter(req(PLAIN), e)).toBe(false);
    expect(await requestIsSupporter(req("nope"), e)).toBe(false);
    expect(await requestIsSupporter(req(), e)).toBe(false);
  });

  it("a live share published by a supporter carries the badge", async () => {
    const e = await env();
    expect(await badgeOf(e, TOKEN)).toBe(true);
    expect(await badgeOf(e, PLAIN)).toBe(false);
    expect(await badgeOf(e)).toBe(false);
  });

  it("a server owned by a supporter is marked", async () => {
    const e = await env();
    const insert =
      "INSERT INTO servers (id, name, region, address, published, created_at, owner_account_id)" +
      " VALUES (?, ?, 'eu', ?, 1, 1, ?)";
    await e.DB.prepare(insert).bind("s1", "Fan's", "1.2.3.4:54210", "acc-1").run();
    await e.DB.prepare(insert).bind("s2", "Other's", "1.2.3.5:54210", "acc-2").run();
    const rows = await e.DB.prepare(
      `SELECT s.id, ${supporterOwnerSql("s.owner_account_id")} AS supporter FROM servers s ORDER BY s.id`,
    ).all<{ id: string; supporter: number }>();
    expect(rows.results).toEqual([
      { id: "s1", supporter: 1 },
      { id: "s2", supporter: 0 },
    ]);
  });
});
