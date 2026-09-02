import { describe, expect, it } from "vitest";
import { compose, nameKey, type LiveRow, type RegistryRow } from "../src/browse";
import { isAddressKey, isReportedLabel, isRiderCount } from "../src/validate";

const registered: RegistryRow = {
  id: "srv-1",
  name: "Frost Racing EU",
  region: "eu-central-1",
  address: "203.0.113.10:54210",
};

function live(server_id: string, riders: number, over: Partial<LiveRow> = {}): LiveRow {
  return { server_id, riders, server_name: null, track: null, ...over };
}

describe("the browser's key fold", () => {
  it("folds a server name the way the app does", () => {
    // `voice::session::room_key`, in TypeScript. If these two ever disagree, a registered
    // server shows nobody on it while a duplicate row beside it shows everybody.
    expect(nameKey("  Frost's  Test Server #2  ")).toBe("frost's test server #2");
  });
});

describe("composing the browser", () => {
  it("counts a registered server's riders however they are keyed", () => {
    // One rider's app resolved the address against the registry before the game was up;
    // the other is in the session and keyed by the server's folded name. Same grid.
    const rows = compose(
      [registered],
      [live("srv-1", 1), live("frost racing eu", 3, { track: "Indiana" })],
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      id: "srv-1",
      name: "Frost Racing EU",
      address: "203.0.113.10:54210",
      riders: 4,
      track: "Indiana",
      registered: true,
    });
  });

  it("lists a registered server nobody is on", () => {
    // An empty server you can join is still somewhere to go.
    const rows = compose([registered], []);
    expect(rows).toHaveLength(1);
    expect(rows[0].riders).toBe(0);
  });

  it("lists a server nobody registered, by the name its riders see", () => {
    const rows = compose(
      [],
      [live("mxb central public", 7, { server_name: "MXB Central Public", track: "Red Bud" })],
    );
    expect(rows[0]).toMatchObject({
      id: "mxb central public",
      name: "MXB Central Public",
      region: null,
      // No address: the app never learns one for a server picked out of the game's browser.
      address: null,
      track: "Red Bud",
      riders: 7,
      registered: false,
    });
  });

  it("falls back to the folded key when nobody reported a name", () => {
    // Every app in a session reports the key; only the ones that can read the game report
    // what it is called. A lowercase name beats no row at all.
    expect(compose([], [live("some community server", 2)])[0].name).toBe("some community server");
  });

  it("never publishes a presence keyed by an address", () => {
    // That key is somebody's server address, and this list is public.
    const rows = compose([], [live("198.51.100.7:54210", 4), live("a real name", 1)]);
    expect(rows.map((s) => s.id)).toEqual(["a real name"]);
  });

  it("puts the busiest servers first, and is stable when nothing is busy", () => {
    const rows = compose([registered], [live("zeta", 1), live("alpha", 1), live("busy place", 9)]);
    expect(rows.map((s) => s.name)).toEqual(["busy place", "alpha", "zeta", "Frost Racing EU"]);
  });
});

describe("what a rider may report about their server", () => {
  it("takes a name and a track, and refuses control characters", () => {
    expect(isReportedLabel("Frost Racing EU")).toBe(true);
    expect(isReportedLabel("mx-bikes")).toBe(false);
    expect(isReportedLabel("   ")).toBe(false);
    expect(isReportedLabel("x".repeat(97))).toBe(false);
  });

  it("takes a head count no bigger than a grid", () => {
    expect(isRiderCount(0)).toBe(true);
    expect(isRiderCount(64)).toBe(true);
    expect(isRiderCount(65)).toBe(false);
    expect(isRiderCount(2.5)).toBe(false);
    expect(isRiderCount("4")).toBe(false);
  });

  it("knows an address key from a server name", () => {
    expect(isAddressKey("203.0.113.10:54210")).toBe(true);
    expect(isAddressKey("mxb.example.com:54210")).toBe(true);
    expect(isAddressKey("frost racing eu")).toBe(false);
    expect(isAddressKey("server #2")).toBe(false);
  });
});
