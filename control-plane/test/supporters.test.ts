import { describe, expect, it } from "vitest";
import { d1 } from "./d1sqlite";
import {
  adminSupporterAction,
  grantSupporter,
  isSupporter,
  listSupporters,
  revokeSupporter,
} from "../src/supporters";

const STEAM = "76561198000000001";
const OTHER = "76561198000000002";

function env(): Env {
  return { DB: d1() } as unknown as Env;
}

describe("supporters", () => {
  it("nobody is a supporter until granted", async () => {
    const e = env();
    expect(await isSupporter(e, STEAM)).toBe(false);
    expect(await isSupporter(e, null)).toBe(false);
  });

  it("a grant makes a supporter, and only that one", async () => {
    const e = env();
    expect(await grantSupporter(e, STEAM, "stripe", "cs_1")).toBe(true);
    expect(await isSupporter(e, STEAM)).toBe(true);
    expect(await isSupporter(e, OTHER)).toBe(false);
  });

  it("the same grant twice is one grant", async () => {
    const e = env();
    await grantSupporter(e, STEAM, "stripe", "cs_1");
    expect(await grantSupporter(e, STEAM, "stripe", "cs_1")).toBe(false);
    expect(await listSupporters(e)).toHaveLength(1);
  });

  it("revoking one grant leaves the others standing", async () => {
    const e = env();
    await grantSupporter(e, STEAM, "stripe", "cs_1");
    await grantSupporter(e, STEAM, "bmac", "hash");
    expect(await revokeSupporter(e, STEAM, "stripe", "cs_1")).toBe(true);
    expect(await isSupporter(e, STEAM)).toBe(true);
    await revokeSupporter(e, STEAM, "bmac");
    expect(await isSupporter(e, STEAM)).toBe(false);
  });

  it("a revoked grant can be granted again", async () => {
    const e = env();
    await grantSupporter(e, STEAM, "stripe", "cs_1");
    await revokeSupporter(e, STEAM, "stripe", "cs_1");
    expect(await grantSupporter(e, STEAM, "stripe", "cs_1")).toBe(true);
    expect(await isSupporter(e, STEAM)).toBe(true);
  });

  it("an admin grants and revokes by Steam id, and is recorded", async () => {
    const e = env();
    const granted = await adminSupporterAction(e, "grant", STEAM, OTHER);
    expect(granted).toEqual({ ok: true, steamId: STEAM, changed: true });
    const [row] = await listSupporters(e);
    expect(row.source).toBe("manual");
    expect(row.grantedBy).toBe(OTHER);
    await adminSupporterAction(e, "revoke", STEAM, OTHER);
    expect(await isSupporter(e, STEAM)).toBe(false);
  });

  it("an admin action it doesn't know is refused", async () => {
    const result = await adminSupporterAction(env(), "promote", STEAM, OTHER);
    expect(result.ok).toBe(false);
  });
});
