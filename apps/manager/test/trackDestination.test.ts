import { expect, test } from "bun:test";
import { buildDestinations, resolveInitialFolder, MOD_TYPES } from "@frost/shared/api/mods";
import type { InstalledMod } from "@frost/shared/api/mods";

// The remembered folder lives in localStorage, which bun does not provide.
const store = new Map<string, string>();
globalThis.localStorage ??= {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
  removeItem: (k: string) => void store.delete(k),
} as Storage;

const tracks = MOD_TYPES.find((mt) => mt.id === "tracks")!;
const game = { id: "mxb" } as Parameters<typeof resolveInitialFolder>[0];
const at = (folder: string) => ({ folder }) as unknown as InstalledMod;
const library = [at("enduro"), at("motocross"), at("supercross"), at("supercross")];

// mxb-mods' categories on KMX-Mesquite MX, as the post lists them.
const mesquite = ["Beta 19", "Intermediate", "Motocross", "Soft Soil", "Tracks"];

test("a track post's category picks the folder of that name", () => {
  const d = buildDestinations(tracks, "KMX-Mesquite MX", library, false, false, mesquite);
  expect(d.guess).toBe("motocross");
  expect(d.derived).toBe(true);
  expect(d.suggestions).toContain("motocross");
});

test("the post's folder outranks the one remembered from the last install", () => {
  localStorage.setItem("frost-dest-tracks", "supercross");
  try {
    const d = buildDestinations(tracks, "KMX-Mesquite MX", library, false, false, mesquite);
    const dest = resolveInitialFolder(game, tracks, d.options, d.guess, false, false, {
      target: null,
      derived: d.derived,
    });
    expect(dest).toBe("motocross");
  } finally {
    localStorage.removeItem("frost-dest-tracks");
  }
});

test("a nested folder matches by its last segment", () => {
  const d = buildDestinations(tracks, "X", [at("2024/Motocross"), at("sx")], false, false, ["Motocross"]);
  expect(d.guess).toBe("2024/Motocross");
});

test("no matching category keeps the memory and the first-folder fallback", () => {
  const d = buildDestinations(tracks, "X", library, false, false, ["Tracks", "Beta 19"]);
  expect(d.derived).toBe(false);
  expect(d.guess).toBe("enduro");
  localStorage.setItem("frost-dest-tracks", "supercross");
  try {
    const dest = resolveInitialFolder(game, tracks, d.options, d.guess, false, false, {
      target: null,
      derived: d.derived,
    });
    expect(dest).toBe("supercross");
  } finally {
    localStorage.removeItem("frost-dest-tracks");
  }
});
