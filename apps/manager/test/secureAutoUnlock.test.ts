import { expect, test } from "bun:test";
import { refreshAfterAutoUnlock } from "../src/lib/secureAutoUnlock";

test("an automatic key grant refreshes the stale library scan", async () => {
  let refreshed = 0;
  await refreshAfterAutoUnlock(async () => 1, () => {
    refreshed += 1;
  });
  expect(refreshed).toBe(1);
});

test("a no-op automatic key pass leaves the library scan alone", async () => {
  let refreshed = 0;
  await refreshAfterAutoUnlock(async () => 0, () => {
    refreshed += 1;
  });
  expect(refreshed).toBe(0);
});
