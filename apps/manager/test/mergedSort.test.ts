import { describe, expect, test } from "bun:test";
import { sortMerged, type MergedMod } from "../src/Components/Mods/sources";

const item = (source: MergedMod["source"], id: number, updated: number): MergedMod =>
  ({
    key: `${source}:${id}`,
    source,
    id,
    title: `${source} ${id}`,
    image: null,
    author: null,
    updated,
    price: null,
    url: null,
  }) as MergedMod;

describe("sortMerged", () => {
  test("most popular keeps each source's own ranking and takes them in turn", () => {
    // Each source's list arrives most popular first; the newest item is the least popular.
    const items = [
      item("mods", 1, 100),
      item("mods", 2, 300),
      item("mods", 3, 900),
      item("hub", 7, 50),
      item("hub", 8, 800),
    ];
    expect(sortMerged(items, "popular").map((m) => m.key)).toEqual([
      "mods:1",
      "hub:7",
      "mods:2",
      "hub:8",
      "mods:3",
    ]);
  });

  test("newest still orders by date", () => {
    const items = [item("mods", 1, 100), item("hub", 7, 500)];
    expect(sortMerged(items, "newest").map((m) => m.key)).toEqual(["hub:7", "mods:1"]);
  });
});
