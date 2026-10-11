import { describe, expect, test } from "bun:test";
import type { HealthReport } from "@frost/shared/types";
import { tyreNotices } from "../src/lib/healthNotice";

function report(): HealthReport {
  return {
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
    tyres: { empty: [], overrides: [] },
  };
}

describe("tyreNotices", () => {
  test("no tyre problems means no bars", () => {
    expect(tyreNotices(report())).toEqual({ empty: null, overrides: null });
  });

  test("empty folders and stock overrides each get their own bar", () => {
    const r = { ...report(), tyres: { empty: ["p_mx", "custom"], overrides: ["m_sm.pkz"] } };
    expect(tyreNotices(r)).toEqual({
      empty: { names: "p_mx, custom", count: 2 },
      overrides: { names: "m_sm.pkz", count: 1 },
    });
  });
});