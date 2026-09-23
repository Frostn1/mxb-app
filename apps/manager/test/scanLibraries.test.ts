import { expect, test } from "bun:test";
import { scanLibrariesSequentially } from "../src/lib/scanLibraries";

test("library roots are scanned sequentially and duplicate roots are collapsed", async () => {
  let running = 0;
  let peak = 0;
  const order: string[] = [];
  const scan = async (subpath: string) => {
    running += 1;
    peak = Math.max(peak, running);
    order.push(subpath);
    await Promise.resolve();
    running -= 1;
    return [];
  };

  await scanLibrariesSequentially(["mods/bikes", "mods/tracks", "mods/bikes"], scan);

  expect(peak).toBe(1);
  expect(order).toEqual(["mods/bikes", "mods/tracks"]);
});
