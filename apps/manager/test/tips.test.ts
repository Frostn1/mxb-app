import { describe, expect, test } from "bun:test";
import { markTipDone, pickTip, TIP_GAP_MS, versionAtLeast, type TipsState } from "../src/lib/tips";

const fresh: TipsState = { done: [], lastShownAt: 0, off: false };
const NOW = 1_800_000_000_000;

describe("pickTip", () => {
  test("picks the first eligible tip in order", () => {
    expect(
      pickTip(
        [
          { id: "a", eligible: false },
          { id: "b", eligible: true },
          { id: "c", eligible: true },
        ],
        fresh,
        NOW,
      ),
    ).toBe("b");
  });

  test("waits while a tip ahead is still being asked", () => {
    expect(
      pickTip(
        [
          { id: "a", eligible: null },
          { id: "b", eligible: true },
        ],
        fresh,
        NOW,
      ),
    ).toBe("wait");
  });

  test("never repeats a tip already done", () => {
    const state = markTipDone(fresh, "a");
    expect(pickTip([{ id: "a", eligible: true }], state, NOW)).toBeNull();
    // ...and a done tip still being asked doesn't hold the others back.
    expect(
      pickTip(
        [
          { id: "a", eligible: null },
          { id: "b", eligible: true },
        ],
        state,
        NOW,
      ),
    ).toBe("b");
  });

  test("shows nothing when tips are off", () => {
    expect(pickTip([{ id: "a", eligible: true }], { ...fresh, off: true }, NOW)).toBeNull();
  });

  test("keeps tips occasional", () => {
    const recent = { ...fresh, lastShownAt: NOW - TIP_GAP_MS + 1000 };
    expect(pickTip([{ id: "a", eligible: true }], recent, NOW)).toBeNull();
    const old = { ...fresh, lastShownAt: NOW - TIP_GAP_MS - 1000 };
    expect(pickTip([{ id: "a", eligible: true }], old, NOW)).toBe("a");
  });

  test("marking done records when it was shown", () => {
    const state = markTipDone(fresh, "a", NOW);
    expect(state.done).toEqual(["a"]);
    expect(state.lastShownAt).toBe(NOW);
    // Dismissing later doesn't move the clock or duplicate the id.
    expect(markTipDone(state, "a")).toEqual(state);
  });
});

describe("versionAtLeast", () => {
  test("reads release tags", () => {
    expect(versionAtLeast("v0.49.9", "0.49.9")).toBe(true);
    expect(versionAtLeast("0.49.10", "0.49.9")).toBe(true);
    expect(versionAtLeast("v0.49.8", "0.49.9")).toBe(false);
    expect(versionAtLeast("v0.50.0-beta.1", "0.49.9")).toBe(true);
  });

  test("unknown is never enough", () => {
    expect(versionAtLeast(null, "0.49.9")).toBe(false);
    expect(versionAtLeast("", "0.49.9")).toBe(false);
    expect(versionAtLeast("dev", "0.49.9")).toBe(false);
  });
});
