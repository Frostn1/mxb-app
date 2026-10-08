import { describe, expect, it } from "vitest";
import { d1 } from "./d1sqlite";
import { putReport } from "../src/diagnostics";

// Obviously fake: nothing here belongs to a real player.
const GUID = "76561197960265730";

async function addAccount(DB: Env["DB"], id: string, name: string) {
  await DB.prepare("INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, ?)")
    .bind(id, name, `hash-${id}`, Date.now())
    .run();
}

function upload(guid: string) {
  return new Request("https://api.test/v1/diagnostics", {
    method: "PUT",
    body: JSON.stringify({
      guid,
      appVersion: "1.0.0",
      modules: [{ name: "mxbikes.exe", origin: "game" }],
    }),
  });
}

const acct = (id: string) => ({ id, rider_name: id, guid: null, steam_id: null, kind: "player" }) as never;

describe("a first diagnostics upload carrying a GUID", () => {
  it("two concurrent first uploads for the same GUID both succeed and are stored", async () => {
    const DB = d1();
    const env = { DB } as unknown as Env;
    await addAccount(DB, "acc-a", "Alpha");
    await addAccount(DB, "acc-b", "Bravo");

    const [a, b] = await Promise.all([
      putReport(upload(GUID), acct("acc-a"), env),
      putReport(upload(GUID), acct("acc-b"), env),
    ]);
    expect(a.status).toBe(200);
    expect(b.status).toBe(200);

    const rows = await DB.prepare("SELECT account_id FROM client_modules ORDER BY account_id").all<{
      account_id: string;
    }>();
    expect((rows.results ?? []).map((r) => r.account_id)).toEqual(["acc-a", "acc-b"]);

    const held = await DB.prepare("SELECT COUNT(*) AS n FROM accounts WHERE guid = ?")
      .bind(GUID)
      .first<{ n: number }>();
    expect(held?.n).toBe(1);
  });

  it("still stores the report when another account already holds the GUID", async () => {
    const DB = d1();
    const env = { DB } as unknown as Env;
    await addAccount(DB, "acc-a", "Alpha");
    await addAccount(DB, "acc-b", "Bravo");
    await DB.prepare("UPDATE accounts SET guid = ? WHERE id = ?").bind(GUID, "acc-a").run();

    const res = await putReport(upload(GUID), acct("acc-b"), env);
    expect(res.status).toBe(200);
    const row = await DB.prepare("SELECT account_id FROM client_modules WHERE account_id = ?")
      .bind("acc-b")
      .first();
    expect(row).not.toBeNull();
  });
});
