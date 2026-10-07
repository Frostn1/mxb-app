import { expect, test } from "bun:test";
import type { ModType } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import { countable } from "../src/Components/Library/countable";

const entry = (name: string, category: string) => ({ name, category }) as LibraryEntry;
const bikes = { id: "bikes" } as ModType;
const tracks = { id: "tracks" } as ModType;

const rows = [
  entry("KTM 450", "bike"),
  entry("Red.pnt", "bikePaint"),
  entry("Yami", "bikeModelSwap"),
];

test("Bikes keeps liveries and model swaps out of its own grid", () => {
  expect(countable(rows, bikes).map((r) => r.name)).toEqual(["KTM 450"]);
});

test("the Liveries view is the one place Bikes lets them through, so they can be uninstalled", () => {
  expect(countable(rows.filter((r) => r.category === "bikePaint"), bikes, true)).toHaveLength(1);
});

test("other types are never filtered", () => {
  expect(countable(rows, tracks)).toHaveLength(3);
});
