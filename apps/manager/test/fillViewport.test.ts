import { describe, expect, test } from "bun:test";
import { needsMoreToFill } from "../src/lib/fillViewport";

const base = { scrollHeight: 600, clientHeight: 1200, hasMore: true, busy: false, blocked: false };

describe("needsMoreToFill", () => {
  test("asks for more while the grid is shorter than the scroller", () => {
    expect(needsMoreToFill(base)).toBe(true);
  });
  test("stops once the scroller overflows", () => {
    expect(needsMoreToFill({ ...base, scrollHeight: 1800 })).toBe(false);
  });
  test("stands down while loading, after a failure, or with no more pages", () => {
    expect(needsMoreToFill({ ...base, busy: true })).toBe(false);
    expect(needsMoreToFill({ ...base, blocked: true })).toBe(false);
    expect(needsMoreToFill({ ...base, hasMore: false })).toBe(false);
  });
  test("ignores a scroller that is not laid out", () => {
    expect(needsMoreToFill({ ...base, clientHeight: 0, scrollHeight: 0 })).toBe(false);
  });
});
