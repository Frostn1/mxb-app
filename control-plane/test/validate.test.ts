import { describe, expect, it } from "vitest";
import { bearer } from "../src/auth";
import {
  isGuid,
  isPaintFileName,
  isPaintSize,
  isRelDest,
  isRiderName,
  isSha256,
  isSlot,
  MAX_PAINT_BYTES,
} from "../src/validate";

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

describe("destination paths", () => {
  it("accepts the layout the game actually uses", () => {
    expect(isRelDest("bikes/2026 KTM 450/paints/Frost.pnt")).toBe(true);
    expect(isRelDest("rider/helmets/Airoh/paints/Frost.pnt")).toBe(true);
  });

  it("refuses to escape the mods folder", () => {
    // One player uploads this and another player's app joins it onto a real directory —
    // this is the value that would actually write outside the mods folder.
    for (const bad of [
      "../mxbikes.ini",
      "bikes/../../../mxbikes.ini",
      "/etc/passwd",
      "C:/Windows/system32/x.pnt",
      "c:\\windows\\x.pnt",
      "bikes\\ktm\\paints\\x.pnt",
      "bikes//paints/x.pnt",
      "./x.pnt",
    ]) {
      expect(isRelDest(bad), bad).toBe(false);
    }
  });

  it("still requires the last segment to be a paint", () => {
    expect(isRelDest("bikes/ktm/paints/notapaint.txt")).toBe(false);
    expect(isRelDest("bikes/ktm/paints")).toBe(false);
  });

  it("rejects control characters and absurd lengths", () => {
    expect(isRelDest("bikes/\u0000/x.pnt")).toBe(false);
    expect(isRelDest("a/".repeat(200) + "x.pnt")).toBe(false);
    expect(isRelDest(42)).toBe(false);
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

describe("player GUIDs", () => {
  it("accepts plausible opaque identifiers", () => {
    expect(isGuid("ab12cd34ef56")).toBe(true);
    expect(isGuid("A1B2-C3D4-E5F6")).toBe(true);
  });

  it("rejects anything with whitespace", () => {
    // The server log delimits the GUID by whitespace, so one containing any could never
    // have come from there.
    expect(isGuid("ab12 cd34")).toBe(false);
    expect(isGuid(" ")).toBe(false);
  });

  it("rejects lengths and characters that are not an identifier", () => {
    expect(isGuid("ab")).toBe(false);
    expect(isGuid("x".repeat(101))).toBe(false);
    expect(isGuid("../etc/passwd")).toBe(false);
    expect(isGuid(null)).toBe(false);
  });
});

describe("slots", () => {
  it("accepts the profile.ini section names the game uses", () => {
    expect(isSlot("paint")).toBe(true);
    expect(isSlot("goggles_paint")).toBe(true);
    expect(isSlot("protection_paint")).toBe(true);
    // Not a paint slot — models and non-paint settings must not carry a blob.
    expect(isSlot("tyres")).toBe(false);
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
