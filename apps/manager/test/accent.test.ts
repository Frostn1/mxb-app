import { expect, test } from "bun:test";
import { PACK_COLORWAYS, accentVars, isPackColorway, parseAccent } from "@frost/shared/lib/accent";

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
