import { describe, expect, test } from "bun:test";
import { BoundedCache } from "../src/lib/boundedCache";

describe("BoundedCache", () => {
  test("evicts the least recently used row at the entry ceiling", () => {
    const cache = new BoundedCache<string>(2, 100);
    cache.set("a", "a", "A", 1);
    cache.set("b", "b", "B", 1);
    expect(cache.get("a")).toBe("A");
    cache.set("c", "c", "C", 1);
    expect(cache.get("b")).toBeUndefined();
    expect(cache.get("a")).toBe("A");
    expect(cache.get("c")).toBe("C");
  });

  test("bounds retained bytes as well as row count", () => {
    const cache = new BoundedCache<string>(10, 8);
    cache.set("a", "a", "A", 5);
    cache.set("b", "b", "B", 5);
    expect(cache.get("a")).toBeUndefined();
    expect(cache.bytes).toBe(5);
  });

  test("a replacement discards the older stamp of the same source", () => {
    const cache = new BoundedCache<string>(10, 100);
    cache.set("track:10", "track", "old", 10);
    cache.set("track:20", "track", "new", 12);
    expect(cache.get("track:10")).toBeUndefined();
    expect(cache.get("track:20")).toBe("new");
    expect(cache.size).toBe(1);
    expect(cache.bytes).toBe(12);
  });

  test("exposes a value-only snapshot without leaking bookkeeping", () => {
    const cache = new BoundedCache<string>(2, 100);
    cache.set("a", "a", "A", 1);
    cache.set("b", "b", "B", 1);
    expect(Object.fromEntries(cache.entries())).toEqual({ a: "A", b: "B" });
  });
});
