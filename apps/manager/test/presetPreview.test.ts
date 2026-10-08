import { expect, test } from "bun:test";
import type { Loadout } from "@frost/shared/types";
import {
  EMPTY_LOADOUT,
  bikeLocked,
  previewIssues,
  securedName,
  type Scans,
} from "@frost/shared/lib/presets";

const scans = { secured: ["locked kit", "oem bike"], tyres: [] } as unknown as Scans;
const loadout: Loadout = {
  ...EMPTY_LOADOUT,
  helmet: "Airoh",
  helmetPaint: "Gone",
  suitPaint: "Locked Kit",
  bikeFont: "NotDrawn",
};

test("a secured file is named the way a preset writes it", () => {
  expect(securedName("Locked Kit.pnt.mxbsecure")).toBe("locked kit");
  expect(securedName("Oem Bike.MXBSECURE")).toBe("oem bike");
});

test("missing and locked parts are told apart, and undrawn slots are ignored", () => {
  const missing = new Set(["helmetPaint", "suitPaint", "bikeFont"]);
  const got = previewIssues(loadout, scans, (s) => missing.has(s.key));
  expect(got.map((i) => [i.slot.key, i.kind])).toEqual([
    ["suitPaint", "locked"],
    ["helmetPaint", "missing"],
  ]);
});

test("nothing is reported before the scan lands", () => {
  expect(previewIssues(loadout, null, () => true)).toEqual([]);
});

test("a bike installed only as locked content is flagged", () => {
  expect(bikeLocked("OEM Bike", scans)).toBe(true);
  expect(bikeLocked("KTM", scans)).toBe(false);
  expect(bikeLocked("", scans)).toBe(false);
});
