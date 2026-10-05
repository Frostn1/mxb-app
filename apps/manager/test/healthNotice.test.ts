import { describe, expect, test } from "bun:test";
import type { HealthReport } from "@frost/shared/types";
import { onedriveNotice, onlineOnlyTotal } from "../src/lib/healthNotice";

function report(over: Partial<HealthReport["onedrive"]> = {}): HealthReport {
  return {
    onedrive: {
      pibosoDir: "C:\\Users\\u\\OneDrive\\Documents\\PiBoSo",
      pibosoInOnedrive: false,
      gameDir: "D:\\SteamLibrary\\steamapps\\common\\MX Bikes",
      gameInOnedrive: false,
      pinned: false,
      onlineOnly: { bikes: 0, tracks: 0, paints: 0, plugins: 0 },
      scanned: 100,
      truncated: false,
      ...over,
    },
    reshade: {
      gameDir: "",
      active: false,
      otherDlls: [],
      disabled: false,
      version: null,
      addons: [],
      config: false,
      log: false,
    },
  };
}

describe("onedriveNotice", () => {
  test("nothing in OneDrive and nothing online-only is no notice", () => {
    expect(onedriveNotice(report())).toBeNull();
  });

  test("online-only files are the loud case, and pinning is offered", () => {
    const r = report({
      pibosoInOnedrive: true,
      onlineOnly: { bikes: 2, tracks: 1, paints: 3, plugins: 0 },
    });
    expect(onlineOnlyTotal(r)).toBe(6);
    expect(onedriveNotice(r)).toEqual({ kind: "online", canKeep: true });
  });

  test("a PiBoSo folder in OneDrive offers pinning until it is pinned", () => {
    expect(onedriveNotice(report({ pibosoInOnedrive: true }))).toEqual({
      kind: "piboso",
      canKeep: true,
    });
    expect(onedriveNotice(report({ pibosoInOnedrive: true, pinned: true }))).toEqual({
      kind: "piboso",
      canKeep: false,
    });
  });

  test("only the game folder in OneDrive has no PiBoSo pin to offer", () => {
    expect(onedriveNotice(report({ gameInOnedrive: true }))).toEqual({
      kind: "game",
      canKeep: false,
    });
  });
});
