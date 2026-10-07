import { expect, test } from "bun:test";
import type { DownloadRecord } from "@frost/shared/types";
import type { DuplicateGroup, StorageMod } from "../src/api/storage";
import { redownloadSources, registerRedownloadResolver } from "../src/lib/redownload";
import { duplicateRemovals, duplicateWaste, totalBytes } from "../src/Components/Storage/selection";

const mod = (name: string, path = `/mods/tracks/${name}`, size = 10): StorageMod => ({
  name,
  path,
  subpath: "mods/tracks",
  category: "track",
  kind: "pkz",
  size,
  modified: 0,
  secured: false,
  lastUsed: null,
});

const record = (title: string, over: Partial<DownloadRecord> = {}): DownloadRecord => ({
  id: title,
  at: 0,
  title,
  slug: "some-slug",
  subpath: "mods/tracks",
  destFolder: "",
  categoryId: null,
  source: "site",
  host: null,
  url: null,
  bytes: null,
  status: "installed",
  ...over,
});

test("a mod downloaded from the catalog can be re-downloaded", () => {
  const mods = [mod("RedBud_2024.pkz"), mod("Hand Made.pkz")];
  const got = redownloadSources(mods, [record("RedBud 2024")]);
  expect(got.get("/mods/tracks/RedBud_2024.pkz")).toEqual({
    kind: "site",
    title: "RedBud 2024",
    slug: "some-slug",
  });
  expect(got.has("/mods/tracks/Hand Made.pkz")).toBe(false);
});

test("failed downloads and dragged-in files are no source", () => {
  const mods = [mod("Washougal.pkz")];
  expect(redownloadSources(mods, [record("Washougal", { status: "failed" })]).size).toBe(0);
  expect(redownloadSources(mods, [record("Washougal", { source: "file", slug: "" })]).size).toBe(0);
  expect(redownloadSources(mods, [record("Washougal", { source: "shop", slug: "" })]).get(
    "/mods/tracks/Washougal.pkz",
  )?.kind).toBe("shop");
});

test("a registered resolver (the mirror hook) answers what history can't", () => {
  const off = registerRedownloadResolver((m) =>
    m.name.startsWith("Hand") ? { kind: "mirror", title: m.name } : null,
  );
  try {
    const got = redownloadSources([mod("Hand Made.pkz")], []);
    expect(got.get("/mods/tracks/Hand Made.pkz")?.kind).toBe("mirror");
  } finally {
    off();
  }
  expect(redownloadSources([mod("Hand Made.pkz")], []).size).toBe(0);
});

test("a duplicate group always keeps exactly one copy", () => {
  const group: DuplicateGroup = {
    id: "x",
    size: 10,
    items: [mod("A.pkz"), mod("B.pkz"), mod("C.pkz")],
  };
  expect(duplicateRemovals(group, "/mods/tracks/B.pkz").map((m) => m.name)).toEqual(["A.pkz", "C.pkz"]);
  // An unknown keep choice falls back to the suggested first copy.
  expect(duplicateRemovals(group, "/elsewhere").map((m) => m.name)).toEqual(["B.pkz", "C.pkz"]);
  expect(duplicateWaste([group])).toBe(20);
  expect(totalBytes(group.items)).toBe(30);
});
