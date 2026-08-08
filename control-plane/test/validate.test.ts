import { describe, expect, it } from "vitest";
import { bearer } from "../src/auth";
import { isPaintFileName, isPaintSize, isRiderName, isSha256, isSlot, MAX_PAINT_BYTES } from "../src/validate";

describe("paint filenames", () => {
  it("accepts an ordinary paint", () => {
    expect(isPaintFileName("Alpinestars.pnt")).toBe(true);
    expect(isPaintFileName("  my paint 2026.PNT  ")).toBe(true);
  });

  it("rejects anything that could escape the paints folder", () => {
    // This value becomes a path on another player's disk, so traversal is the one that
    // actually matters — everything else here is defence in depth.
    for (const bad of [
      "../../mxbikes.ini",
      "..\\..\\core.ini",
      "sub/dir.pnt",
      "sub\\dir.pnt",
      "..pnt.pnt/../x.pnt",
    ]) {
      expect(isPaintFileName(bad), bad).toBe(false);
    }
  });

  it("rejects names Windows cannot hold, and control characters", () => {
    for (const bad of ['a:b.pnt', 'a*b.pnt', 'a?b.pnt', 'a"b.pnt', "a<b.pnt", "a>b.pnt", "a|b.pnt", "a\u0000b.pnt"]) {
      expect(isPaintFileName(bad), JSON.stringify(bad)).toBe(false);
    }
  });

  it("requires the extension the game actually loads", () => {
    expect(isPaintFileName("livery.png")).toBe(false);
    expect(isPaintFileName("livery")).toBe(false);
    expect(isPaintFileName("")).toBe(false);
  });
});

describe("rider names", () => {
  it("accepts names the roster could report back", () => {
    expect(isRiderName("Frost")).toBe(true);
    expect(isRiderName("Jean-Luc #47")).toBe(true);
  });

  it("rejects control characters, since the name must survive the round trip", () => {
    expect(isRiderName("Frost\u0000")).toBe(false);
    expect(isRiderName("Frost\u001b[31m")).toBe(false);
  });

  it("rejects lengths that are not a real rider", () => {
    expect(isRiderName("a")).toBe(false);
    expect(isRiderName("x".repeat(65))).toBe(false);
    expect(isRiderName(42)).toBe(false);
  });
});

describe("content addressing", () => {
  it("accepts a lowercase sha-256 and nothing else", () => {
    expect(isSha256("a".repeat(64))).toBe(true);
    expect(isSha256("A".repeat(64))).toBe(false);
    expect(isSha256("a".repeat(63))).toBe(false);
    expect(isSha256("../etc/passwd")).toBe(false);
  });

  it("bounds paint sizes", () => {
    expect(isPaintSize(1)).toBe(true);
    expect(isPaintSize(MAX_PAINT_BYTES)).toBe(true);
    expect(isPaintSize(MAX_PAINT_BYTES + 1)).toBe(false);
    expect(isPaintSize(0)).toBe(false);
    expect(isPaintSize(-1)).toBe(false);
    expect(isPaintSize(1.5)).toBe(false);
  });
});

describe("slots", () => {
  it("accepts the game's slots only", () => {
    expect(isSlot("bike")).toBe(true);
    expect(isSlot("goggles")).toBe(true);
    expect(isSlot("wheels")).toBe(false);
    expect(isSlot(null)).toBe(false);
  });
});

describe("bearer parsing", () => {
  it("takes the token and tolerates case and spacing", () => {
    expect(bearer("Bearer abc123")).toBe("abc123");
    expect(bearer("bearer   abc123  ")).toBe("abc123");
  });

  it("rejects other header shapes", () => {
    expect(bearer("Basic abc123")).toBe(null);
    expect(bearer("Bearer")).toBe(null);
    expect(bearer("Bearer   ")).toBe(null);
    expect(bearer(null)).toBe(null);
  });
});
