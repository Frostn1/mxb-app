import { describe, expect, test } from "bun:test";
import { formatBytes } from "@frost/shared/lib/mods";

describe("formatBytes as shown for a download size", () => {
  test("megabytes and gigabytes", () => {
    expect(formatBytes(52_428_800)).toBe("50 MB");
    expect(formatBytes(1_288_490_189)).toBe("1.2 GB");
  });
  test("an unknown size shows nothing", () => {
    expect(formatBytes(0)).toBe("");
    expect(formatBytes(-1)).toBe("");
  });
});
