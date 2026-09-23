import { describe, expect, test } from "bun:test";
import { AsyncPollingStore } from "../src/lib/asyncPollingStore";

const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
};

describe("AsyncPollingStore", () => {
  test("many subscribers share one in-flight probe", async () => {
    const wait = deferred<boolean>();
    let calls = 0;
    const store = new AsyncPollingStore(false, false, 60_000, () => {
      calls += 1;
      return wait.promise;
    });
    const offA = store.subscribe(() => {});
    const offB = store.subscribe(() => {});
    const manual = store.refresh();
    expect(calls).toBe(1);
    wait.resolve(true);
    await manual;
    expect(store.getSnapshot()).toBe(true);
    offA();
    offB();
  });

  test("a failed probe publishes the fallback without overlapping", async () => {
    let active = 0;
    let peak = 0;
    const store = new AsyncPollingStore(true, false, 60_000, async () => {
      active += 1;
      peak = Math.max(peak, active);
      await Promise.resolve();
      active -= 1;
      throw new Error("offline");
    });
    await Promise.all([store.refresh(), store.refresh(), store.refresh()]);
    expect(peak).toBe(1);
    expect(store.getSnapshot()).toBe(false);
  });
});
