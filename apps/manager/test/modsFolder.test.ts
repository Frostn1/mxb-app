import { expect, test } from "bun:test";
import type { GameFolderCheck } from "@frost/shared/api/mods";
import { defaultFolderInUse } from "../src/lib/modsFolder";

const check = (over: Partial<GameFolderCheck>): GameFolderCheck => ({
  path: "D:\Games\MXB",
  correction: null,
  expected: "C:\Users\r\OneDrive\Documents\PiBoSo\MX Bikes",
  matchesExpected: false,
  exists: true,
  hasMods: true,
  hasProfiles: true,
  isModsTree: false,
  usable: true,
  ...over,
});

test("a saved folder that is not the default, with a real default folder beside it, warns", () => {
  expect(defaultFolderInUse(check({}), check({ matchesExpected: true }))).toBe(
    "C:\Users\r\OneDrive\Documents\PiBoSo\MX Bikes",
  );
});

test("the default folder itself never warns", () => {
  expect(defaultFolderInUse(check({ matchesExpected: true }), check({}))).toBeNull();
});

test("a relocation with no real default folder is left alone", () => {
  expect(defaultFolderInUse(check({}), check({ exists: false, usable: false }))).toBeNull();
  expect(defaultFolderInUse(check({}), null)).toBeNull();
});

test("an unusable saved folder is the other warning's job", () => {
  expect(defaultFolderInUse(check({ usable: false }), check({}))).toBeNull();
});
