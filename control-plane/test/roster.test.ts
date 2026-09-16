import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  KEEP_MS,
  MAX_ADDRESSES,
  MAX_REPORTS_PER_DAY,
  MIN_REPORTERS,
  claimRoster,
  parseReport,
  pruneRoster,
  readRoster,
  reportRoster,
} from "../src/roster";
import { ipDigest } from "../src/voice";
import { d1 } from "./d1sqlite";

let e: Env;
let clock = 1_800_000_000_000;

const A = "203.0.113.10:54210";
const B = "198.51.100.4:54210";

/** A report from one network. Different `who` means a different reporter digest. */
async function report(addresses: string[], who = 1): Promise<Response> {
  return reportRoster(
    new Request("https://cp.invalid/v1/roster", {
      method: "POST",
      headers: { "CF-Connecting-IP": `192.0.2.${who}` },
      body: JSON.stringify({ addresses }),
    }),
    e,
  );
}

async function served(): Promise<string[]> {
  const res = await readRoster(e);
  expect(res.status).toBe(200);
  const body = (await res.json()) as { addresses: string[] };
  return body.addresses.sort();
}

/** A real account row: `owner_account` is a foreign key, and the tests hold it to that. */
async function account(id: string): Promise<string> {
  await e.DB.prepare(
    "INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, 0)",
  )
    .bind(id, `rider-${id}`, `hash-${id}`)
    .run();
  return id;
}

/** Enough independent reporters to put an address on the list. */
async function corroborate(address: string): Promise<void> {
  for (let who = 1; who <= MIN_REPORTERS; who += 1) await report([address], who);
}

beforeEach(() => {
  e = { DB: d1() } as unknown as Env;
  vi.useFakeTimers();
  vi.setSystemTime(clock);
});

afterEach(() => {
  vi.useRealTimers();
  clock += 24 * 60 * 60 * 1000;
});

describe("what a report may say", () => {
  it("takes a list of public host:port", () => {
    expect(parseReport(JSON.stringify({ addresses: [A, B] }))).toEqual([A, B]);
  });

  it("folds a repeat inside one report", () => {
    expect(parseReport(JSON.stringify({ addresses: [A, A, A] }))).toEqual([A]);
  });

  it("drops what it can't serve instead of refusing the whole list", () => {
    // One IPv6-only row must not cost a player their other 200 addresses.
    const mixed = [A, "[2001:db8::1]:54210", "not-an-address", B];
    expect(parseReport(JSON.stringify({ addresses: mixed }))).toEqual([A, B]);
  });

  it("refuses a report with nothing usable in it", () => {
    expect(parseReport(JSON.stringify({ addresses: ["nonsense"] }))).toBe(
      "no usable addresses in that report",
    );
  });

  it("refuses a list longer than any real one", () => {
    const many = Array.from({ length: MAX_ADDRESSES + 1 }, (_, i) => `203.0.113.${i % 250}:${54000 + i}`);
    expect(parseReport(JSON.stringify({ addresses: many }))).toMatch(/at most/);
  });

  it("refuses something that isn't a list at all", () => {
    expect(parseReport(JSON.stringify({ addresses: A }))).toBe("addresses must be an array");
    expect(parseReport("not json")).toBe("expected a JSON body");
  });
});

describe("what never reaches the list", () => {
  // This is the security property, not a validation nicety: every address served here is an
  // address thousands of apps will send a datagram to.
  const dangerous = [
    ["loopback", "127.0.0.1:54210"],
    ["private space", "192.168.1.10:54210"],
    ["private space", "10.0.0.5:54210"],
    ["private space", "172.16.4.4:54210"],
    ["cloud metadata", "169.254.169.254:80"],
    ["carrier NAT", "100.64.0.1:54210"],
    ["multicast", "239.255.255.250:1900"],
    ["localhost by name", "localhost:54210"],
    ["no port at all", "203.0.113.10"],
  ] as const;

  for (const [what, address] of dangerous) {
    it(`refuses ${what} (${address})`, () => {
      expect(parseReport(JSON.stringify({ addresses: [address] }))).toBe(
        "no usable addresses in that report",
      );
    });
  }
});

describe("corroboration", () => {
  it("does not serve an address one reporter named", async () => {
    // The whole attack: one POST naming a victim, and every app in the world probes them.
    await report([A], 1);
    expect(await served()).toEqual([]);
  });

  it("does not let one reporter clear the bar by asking twice", async () => {
    for (let i = 0; i < 10; i += 1) await report([A], 1);
    expect(await served()).toEqual([]);
  });

  it("serves it once distinct networks agree", async () => {
    await corroborate(A);
    expect(await served()).toEqual([A]);
  });

  it("holds each address to the bar on its own", async () => {
    await corroborate(A);
    await report([A, B], 1);
    expect(await served()).toEqual([A]);
  });

  it("keeps an address served once it has earned it", async () => {
    // Sticky on purpose: corroboration is a fact about the past, and an address that quietly
    // stopped qualifying would vanish from every install's book at once.
    await corroborate(A);
    vi.setSystemTime(clock + 2 * 24 * 60 * 60 * 1000);
    await report([A], 9);
    expect(await served()).toEqual([A]);
  });

  it("counts reporters within a day, not across them", async () => {
    // The reporter digest is day-salted, so the same network hashes differently tomorrow.
    // Counting across days would read one persistent reporter as several and hand the
    // injection straight back.
    await report([A], 1);
    vi.setSystemTime(clock + 24 * 60 * 60 * 1000);
    await report([A], 1);
    expect(await served()).toEqual([]);
  });
});

describe("the served list", () => {
  it("is empty rather than absent with nothing in it", async () => {
    expect(await served()).toEqual([]);
  });

  it("folds in servers registered through our own registry", async () => {
    // A caller asking what servers exist shouldn't have to know we keep two lists.
    await e.DB.prepare(
      "INSERT INTO servers (id, name, region, address, published, created_at)" +
        " VALUES ('s1', 'Frost EU', 'eu-west', ?, 1, 0)",
    )
      .bind(B)
      .run();
    await corroborate(A);
    expect(await served()).toEqual([A, B].sort());
  });

  it("names an address once even when both lists hold it", async () => {
    await e.DB.prepare(
      "INSERT INTO servers (id, name, region, address, published, created_at)" +
        " VALUES ('s1', 'Frost EU', 'eu-west', ?, 1, 0)",
    )
      .bind(A)
      .run();
    await corroborate(A);
    expect(await served()).toEqual([A]);
  });

  it("is open to anything that wants to read it", async () => {
    const res = await readRoster(e);
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
    expect(res.headers.get("cache-control")).toContain("max-age");
  });

  it("forgets an address nobody has seen in a month", async () => {
    await corroborate(A);
    vi.setSystemTime(clock + KEEP_MS + 60_000);
    expect(await served()).toEqual([]);
  });
});

describe("an operator adding their own server", () => {
  it("is served at once, with no corroborating", async () => {
    // The account is the corroboration, and unlike a sighting it is recorded against them.
    await account("acct-1");
    const res = await claimRoster(
      new Request("https://cp.invalid/v1/roster/mine", {
        method: "POST",
        body: JSON.stringify({ address: B }),
      }),
      "acct-1",
      e,
    );
    expect(res.status).toBe(202);
    expect(await served()).toEqual([B]);
  });

  it("is refused an address the game could never be pointed at", async () => {
    await account("acct-1");
    const res = await claimRoster(
      new Request("https://cp.invalid/v1/roster/mine", {
        method: "POST",
        body: JSON.stringify({ address: "127.0.0.1:54210" }),
      }),
      "acct-1",
      e,
    );
    expect(res.status).toBe(400);
    expect(await served()).toEqual([]);
  });

  it("leaves an already-corroborated address the moment it earned that", async () => {
    await account("acct-1");
    await corroborate(A);
    const earned = await e.DB.prepare("SELECT corroborated_at FROM server_roster WHERE address = ?")
      .bind(A)
      .first<{ corroborated_at: number }>();

    vi.setSystemTime(clock + 60 * 60 * 1000);
    await claimRoster(
      new Request("https://cp.invalid/v1/roster/mine", {
        method: "POST",
        body: JSON.stringify({ address: A }),
      }),
      "acct-1",
      e,
    );
    const after = await e.DB.prepare(
      "SELECT corroborated_at, owner_account FROM server_roster WHERE address = ?",
    )
      .bind(A)
      .first<{ corroborated_at: number; owner_account: string }>();
    expect(after?.corroborated_at).toBe(earned?.corroborated_at);
    expect(after?.owner_account).toBe("acct-1");
  });
});

describe("the endpoint itself", () => {
  it("answers a report with 202", async () => {
    const res = await report([A, B]);
    expect(res.status).toBe(202);
    expect(await res.json()).toEqual({ ok: true, known: 0, pending: 2 });
  });

  it("takes the cheap path for an address it already serves", async () => {
    // The steady state: a 300-server list from an app with the tab open, every few minutes.
    // Everything already known must cost a `last_seen` bump and nothing else.
    await corroborate(A);
    const res = await report([A], 7);
    expect(await res.json()).toEqual({ ok: true, known: 1, pending: 0 });
    const sightings = await e.DB.prepare(
      "SELECT COUNT(*) AS n FROM server_sightings WHERE address = ?",
    )
      .bind(A)
      .first<{ n: number }>();
    expect(sightings?.n).toBe(MIN_REPORTERS);
  });

  it("refuses a body far larger than a report could be", async () => {
    const res = await reportRoster(
      new Request("https://cp.invalid/v1/roster", {
        method: "POST",
        headers: { "content-length": String(64 * 1024) },
        body: JSON.stringify({ addresses: [A] }),
      }),
      e,
    );
    expect(res.status).toBe(413);
  });

  it("caps one address for the day", async () => {
    const day = new Date(clock).toISOString().slice(0, 10);
    await e.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
        " VALUES (?, ?, 'roster', ?, 0)",
    )
      .bind(await ipDigest("192.0.2.3", day, e), day, MAX_REPORTS_PER_DAY)
      .run();
    expect((await report([A], 3)).status).toBe(429);
  });
});

describe("the sweep", () => {
  it("drops addresses past retention and yesterday's sightings", async () => {
    await report([A], 1);
    await corroborate(B);

    vi.setSystemTime(clock + KEEP_MS + 60_000);
    await pruneRoster(e);

    const rows = await e.DB.prepare("SELECT address FROM server_roster").all<{ address: string }>();
    expect(rows.results ?? []).toEqual([]);
    const sightings = await e.DB.prepare("SELECT COUNT(*) AS n FROM server_sightings").first<{
      n: number;
    }>();
    expect(sightings?.n).toBe(0);
  });

  it("keeps what is still being reported", async () => {
    await corroborate(A);
    await pruneRoster(e);
    expect(await served()).toEqual([A]);
  });
});
