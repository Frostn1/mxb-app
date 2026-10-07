import { describe, expect, test } from "bun:test";
import { createMissReporter, MAX_QUERY_CHARS, normaliseMissQuery } from "../src/lib/searchMiss";

describe("normaliseMissQuery", () => {
  test("trims, lowercases and collapses whitespace", () => {
    expect(normaliseMissQuery("  Honda\tCR250 \n 2022 ")).toBe("honda cr250 2022");
  });

  test("caps the length", () => {
    expect(normaliseMissQuery("a".repeat(500))?.length).toBe(MAX_QUERY_CHARS);
  });

  test("a keystroke or blank is not a search", () => {
    expect(normaliseMissQuery("   ")).toBeNull();
    expect(normaliseMissQuery("a")).toBeNull();
  });
});

describe("createMissReporter", () => {
  test("sends the normalised text once per session per scope", () => {
    const sent: string[] = [];
    const report = createMissReporter((q) => sent.push(q));
    expect(report("  Foo Bar ", 1)).toBe(true);
    expect(report("foo bar", 1)).toBe(false);
    expect(report("FOO BAR", 2)).toBe(true);
    expect(sent).toEqual(["foo bar", "foo bar"]);
  });

  test("sends nothing for an unusable query", () => {
    const sent: string[] = [];
    const report = createMissReporter((q) => sent.push(q));
    expect(report(" ")).toBe(false);
    expect(sent).toEqual([]);
  });
});
