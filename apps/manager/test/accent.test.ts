import { expect, test } from "bun:test";
import {
  PACK_COLORWAYS,
  accentVars,
  colorwayUnlocked,
  drawnColorway,
  isFreeNow,
  isPackColorway,
  nextFreeEnd,
  parseAccent,
} from "@frost/shared/lib/accent";

/** The free colorways, as `Context/Theme.tsx` lists them. */
const FREE = ["frost", "ember", "moss", "violet", "rose", "slate", "retro"];

test("only #rrggbb is an accent", () => {
  expect(parseAccent("#FF4A3D")).toBe("#ff4a3d");
  expect(parseAccent(" #0a84ff ")).toBe("#0a84ff");
  expect(parseAccent("red")).toBeNull();
  expect(parseAccent("#fff")).toBeNull();
  expect(parseAccent(null)).toBeNull();
});

test("text on a pale accent is dark, on a deep one light", () => {
  expect(accentVars("#c6f432", "dark")["--primary-foreground"]).toBe("#111111");
  expect(accentVars("#1a237e", "dark")["--primary-foreground"]).toBe("#ffffff");
  expect(accentVars("#ff4a3d", "light")["--primary"]).toBe("#ff4a3d");
});

test("the pack's colorways never join the free list", () => {
  for (const c of PACK_COLORWAYS) {
    expect(isPackColorway(c)).toBe(true);
    expect(FREE.includes(c)).toBe(false);
  }
  for (const c of FREE) expect(isPackColorway(c)).toBe(false);
});

test("Halloween is a Style Pack theme", () => {
  expect(isPackColorway("halloween")).toBe(true);
});

test("Halloween is free for everyone through Oct 31, local time", () => {
  expect(isFreeNow("halloween", new Date(2026, 9, 19, 12))).toBe(true);
  expect(isFreeNow("halloween", new Date(2026, 9, 31, 23, 59, 59))).toBe(true);
  expect(isFreeNow("halloween", new Date(2026, 10, 1, 0, 0, 0))).toBe(false);
  expect(isFreeNow("halloween", new Date(2027, 9, 20))).toBe(false);
  // The rest of the pack stays the pack's.
  for (const c of ["night", "works", "lime", "frost"]) {
    expect(isFreeNow(c, new Date(2026, 9, 25))).toBe(false);
  }
});

test("a free colorway is usable without the pack only while it is free", () => {
  const during = new Date(2026, 9, 25);
  const after = new Date(2026, 10, 1, 0, 0, 1);
  expect(colorwayUnlocked("halloween", false, during)).toBe(true);
  expect(colorwayUnlocked("halloween", false, after)).toBe(false);
  expect(colorwayUnlocked("halloween", true, after)).toBe(true);
  expect(colorwayUnlocked("night", false, during)).toBe(false);
  expect(colorwayUnlocked("ember", false, after)).toBe(true);
});

test("after the free week the pick falls back to the theme before it", () => {
  const during = new Date(2026, 9, 25);
  const after = new Date(2026, 10, 2);
  expect(drawnColorway("halloween", false, during, "moss")).toBe("halloween");
  expect(drawnColorway("halloween", false, after, "moss")).toBe("moss");
  expect(drawnColorway("halloween", false, after, null)).toBe("frost");
  expect(drawnColorway("halloween", true, after, "moss")).toBe("halloween");
  expect(drawnColorway("night", false, during, "moss")).toBe("frost");
});

test("the window is told when the free week ends", () => {
  expect(nextFreeEnd(new Date(2026, 9, 25))?.getTime()).toBe(new Date(2026, 10, 1).getTime());
  expect(nextFreeEnd(new Date(2026, 10, 1))).toBeNull();
});
