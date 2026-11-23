import { describe, expect, it } from "vitest";
import { presence, presenceLabel, type NativeStatus } from "./api";

const base = { active_sessions: 6 } as NativeStatus;

describe("presence", () => {
  it("splits humans from bots", () => {
    const p = presence({ ...base, humans: 0, bots: 6 });
    expect(p).toEqual({ riders: 0, bots: 6 });
    expect(presenceLabel(p)).toBe("0 riders · 6 bots");
  });
  it("hides bots when there are none", () => {
    expect(presenceLabel({ riders: 1, bots: 0 })).toBe("1 rider");
  });
  it("falls back on older servers", () => {
    expect(presence(base)).toEqual({ riders: 6, bots: 0 });
  });
});
