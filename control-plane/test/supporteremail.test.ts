import { describe, expect, it } from "vitest";
import { d1 } from "./d1sqlite";
import { isSupporter } from "../src/supporters";
import {
  confirmEmailCode,
  emailHash,
  normaliseEmail,
  recordBmacPayer,
  sendEmailCode,
  type Mailer,
} from "../src/supporteremail";

const STEAM = "76561198000000001";
const ACCOUNT = { id: "acc-1", steam_id: STEAM };
const NOW = 1_800_000_000_000;

async function env(): Promise<Env> {
  const DB = d1();
  await DB.prepare(
    "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at) VALUES (?, ?, ?, ?, ?)",
  )
    .bind("acc-1", "Fan", STEAM, "hash-1", 1)
    .run();
  return { DB } as unknown as Env;
}

/** A mailer that keeps what it sent, so the test can type the code back. */
function inbox(): { sent: { to: string; code: string }[]; mailer: Mailer } {
  const sent: { to: string; code: string }[] = [];
  return { sent, mailer: async (to, code) => (sent.push({ to, code }), true) };
}

function post(body: unknown): Request {
  return new Request("https://cp.invalid/", { method: "POST", body: JSON.stringify(body) });
}

describe("supporter email", () => {
  it("reads an address the way it was typed, and refuses what isn't one", () => {
    expect(normaliseEmail("  Fan@Example.COM ")).toBe("fan@example.com");
    expect(normaliseEmail("not an email")).toBeNull();
    expect(normaliseEmail(42)).toBeNull();
  });

  it("without a mail provider, says so", async () => {
    const r = await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, await env(), null, NOW);
    expect(r.status).toBe(503);
  });

  it("needs a Steam sign-in, since the pack is kept by Steam id", async () => {
    const e = await env();
    const r = await sendEmailCode(post({ email: "a@b.co" }), { id: "acc-1", steam_id: null }, e, inbox().mailer, NOW);
    expect(r.status).toBe(409);
  });

  it("a coffee first, then the address confirmed: the pack", async () => {
    const e = await env();
    await recordBmacPayer(e, "Fan@example.com", NOW);
    const box = inbox();
    expect((await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW)).status).toBe(200);
    expect(box.sent[0].to).toBe("fan@example.com");
    const r = await confirmEmailCode(post({ code: box.sent[0].code }), ACCOUNT, e, NOW + 1000);
    expect(r).toEqual({ status: 200, body: { ok: true, supporter: true } });
    expect(await isSupporter(e, STEAM)).toBe(true);
  });

  it("the address confirmed first, then a coffee: the pack", async () => {
    const e = await env();
    const box = inbox();
    await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW);
    const r = await confirmEmailCode(post({ code: box.sent[0].code }), ACCOUNT, e, NOW + 1000);
    expect(r.body.supporter).toBe(false);
    await recordBmacPayer(e, "fan@example.com", NOW + 2000);
    expect(await isSupporter(e, STEAM)).toBe(true);
  });

  it("a confirmed address that never paid grants nothing", async () => {
    const e = await env();
    await recordBmacPayer(e, "someone@else.com", NOW);
    const box = inbox();
    await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW);
    await confirmEmailCode(post({ code: box.sent[0].code }), ACCOUNT, e, NOW + 1000);
    expect(await isSupporter(e, STEAM)).toBe(false);
  });

  it("stores hashes, never the address", async () => {
    const e = await env();
    await recordBmacPayer(e, "fan@example.com", NOW);
    const row = await e.DB.prepare("SELECT email_hash FROM bmac_supporters").first<{ email_hash: string }>();
    expect(row?.email_hash).toBe(await emailHash("fan@example.com"));
    expect(row?.email_hash).not.toContain("@");
  });

  it("a wrong code counts, and five of them end the code", async () => {
    const e = await env();
    const box = inbox();
    await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW);
    const wrong = box.sent[0].code === "000000" ? "111111" : "000000";
    for (let i = 0; i < 5; i++) {
      expect((await confirmEmailCode(post({ code: wrong }), ACCOUNT, e, NOW)).status).toBe(400);
    }
    expect((await confirmEmailCode(post({ code: box.sent[0].code }), ACCOUNT, e, NOW)).status).toBe(410);
  });

  it("a code stops working after 15 minutes", async () => {
    const e = await env();
    const box = inbox();
    await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW);
    const late = await confirmEmailCode(post({ code: box.sent[0].code }), ACCOUNT, e, NOW + 16 * 60 * 1000);
    expect(late.status).toBe(410);
  });

  it("three codes an hour, then a wait", async () => {
    const e = await env();
    const box = inbox();
    for (let i = 0; i < 3; i++) {
      expect((await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW + i)).status).toBe(200);
    }
    expect((await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW + 10)).status).toBe(429);
    const later = await sendEmailCode(post({ email: "fan@example.com" }), ACCOUNT, e, box.mailer, NOW + 61 * 60 * 1000);
    expect(later.status).toBe(200);
  });
});
