import { expect, test } from "bun:test";
import type { MasterServer } from "@frost/shared/api/mods";
import { classLine, serverClasses, serverMatchesQuery } from "@/lib/serverClasses";

// `[event] category` exactly as live servers sent it in a GETINFO reply (2026-10-06), split
// on `/` the way the backend's `split_list` does.
const MXB_NATIVE = "MX1/MX1 OEM/MX2/MX2 OEM".split("/");
const TEN_CLASSES =
  "MX1 OEM/MX2 OEM/MX3 OEM/MX1-2T OEM/MX2-2T OEM/Classic MX1 OEM/Classic MX2 OEM/MX-E OEM/MX1/MX2".split(
    "/",
  );

const row = (categories: string[]) =>
  ({ name: "Some Server", track: "Farm 14", location: "USA", address: "", categories }) as MasterServer;

test("every class of a multi-class server is shown, in order", () => {
  expect(serverClasses(MXB_NATIVE)).toEqual(["MX1", "MX1 OEM", "MX2", "MX2 OEM"]);
  expect(classLine(MXB_NATIVE)).toBe("MX1 · MX1 OEM · MX2 · MX2 OEM");
  expect(serverClasses(TEN_CLASSES)).toHaveLength(10);
  expect(classLine(TEN_CLASSES)).toContain("MX-E OEM");
});

test("an open server shows no class line; blanks and repeats are dropped", () => {
  expect(classLine([])).toBe("");
  expect(serverClasses([" MX2 OEM ", "", "mx2 oem", "MX1"])).toEqual(["MX2 OEM", "MX1"]);
});

test("search finds a server by any of its classes, not just the first", () => {
  expect(serverMatchesQuery(row(MXB_NATIVE), "mx2 oem")).toBe(true);
  expect(serverMatchesQuery(row(TEN_CLASSES), "mx-e")).toBe(true);
  expect(serverMatchesQuery(row(MXB_NATIVE), "mx3")).toBe(false);
  expect(serverMatchesQuery(row([]), "farm")).toBe(true);
});
