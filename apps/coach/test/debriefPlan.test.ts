import { describe, expect, test } from "bun:test";
import { planDebrief, withTimeout } from "../src/Components/Sessions/debriefPlan";

const lap = (num: number, timeMs: number, over: Partial<{ whole: boolean; invalid: boolean }> = {}) => ({
  path: "s.mxbc",
  num,
  timeMs,
  whole: true,
  invalid: false,
  ...over,
});

describe("planDebrief", () => {
  test("picks the fastest whole valid lap", () => {
    const p = planDebrief([lap(0, 70000), lap(1, 65000), lap(2, 60000, { invalid: true })]);
    expect(p.best?.num).toBe(1);
    expect(p.missing).toBeNull();
  });
  test("a session with only partial laps is thin, not loading forever", () => {
    const p = planDebrief([lap(0, 0, { whole: false }), lap(1, 0, { whole: false })]);
    expect(p.best).toBeNull();
    expect(p.missing).toBe("noWholeLap");
  });
  test("no laps at all", () => {
    expect(planDebrief([]).missing).toBe("noLaps");
  });
});

describe("withTimeout", () => {
  test("rejects a promise that never settles", async () => {
    await expect(withTimeout(new Promise(() => {}), 20, "slow")).rejects.toThrow("slow");
  });
  test("passes through a result", async () => {
    expect(await withTimeout(Promise.resolve(3), 50, "slow")).toBe(3);
  });
});
