import { describe, expect, test } from "bun:test";
import {
  reviewOutcomeStage,
  TIMED_OUT,
  withTimeout,
} from "../src/lib/installQueue";

describe("reviewOutcomeStage", () => {
  test("an outcome that installed something is done", () => {
    expect(reviewOutcomeStage({ kind: "installed", installed: 1, failed: 2 })).toEqual({
      stage: "done",
    });
  });

  test("an outcome with only failures keeps its first error", () => {
    expect(
      reviewOutcomeStage({
        kind: "installed",
        installed: 0,
        failed: 2,
        firstError: "disk full",
      }),
    ).toEqual({ stage: "error", message: "disk full" });
  });

  test("a discarded review is removed", () => {
    expect(reviewOutcomeStage({ kind: "discarded" })).toEqual({ remove: true });
  });
});

describe("withTimeout", () => {
  test("returns a prompt value", async () => {
    expect(await withTimeout(Promise.resolve(7), 20)).toBe(7);
  });

  test("returns the timeout sentinel for a slow promise", async () => {
    const slow = new Promise<number>((resolve) => setTimeout(() => resolve(7), 30));
    expect(await withTimeout(slow, 1)).toBe(TIMED_OUT);
  });
});
