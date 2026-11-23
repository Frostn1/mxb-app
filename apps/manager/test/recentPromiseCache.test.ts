import { describe, expect, test } from "bun:test";
import { RecentPromiseCache } from "@frost/shared/lib/recentPromiseCache";

describe("RecentPromiseCache", () => {
  test("reuses an in-flight and recent promise until its TTL expires", async () => {
    const cache = new RecentPromiseCache<string, number>(10_000);
    let loads = 0;
    const load = () => Promise.resolve(++loads);

    const first = cache.get("tracks", load, 1_000);
    expect(cache.get("tracks", load, 2_000)).toBe(first);
    expect(await first).toBe(1);
    expect(await cache.get("tracks", load, 10_999)).toBe(1);
    expect(await cache.get("tracks", load, 11_000)).toBe(2);
  });

  test("an in-flight read stays shared even after the TTL", async () => {
    const cache = new RecentPromiseCache<string, number>(10_000);
    let finish!: (value: number) => void;
    const first = cache.get(
      "tracks",
      () => new Promise<number>((resolve) => (finish = resolve)),
      1_000,
    );
    expect(cache.get("tracks", async () => 2, 20_000)).toBe(first);
    finish(1);
    expect(await first).toBe(1);
  });

  test("clear and rejection both allow the next read to load again", async () => {
    const cache = new RecentPromiseCache<string, number>(10_000);
    let loads = 0;
    const rejected = cache.get("bikes", () => {
      loads += 1;
      return Promise.reject(new Error("scan failed"));
    });
    await expect(rejected).rejects.toThrow("scan failed");
    expect(await cache.get("bikes", async () => ++loads)).toBe(2);

    cache.clear();
    expect(await cache.get("bikes", async () => ++loads)).toBe(3);
  });
});
