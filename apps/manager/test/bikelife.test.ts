import { expect, test } from "bun:test";
import {
  AUTO_SUBPATH,
  MOD_TYPES,
  entrySubpath,
  librarySubpaths,
  modTypesFor,
  purchaseModTypes,
  routesByContent,
  scanSubpaths,
} from "@frost/shared/api/mods";

const bikelife = MOD_TYPES.find((mt) => mt.id === "bikelife")!;
const bikes = MOD_TYPES.find((mt) => mt.id === "bikes")!;

test("Bikelife is mxb-mods' own category, sorted by content", () => {
  expect(bikelife.categoryId).toBe(175);
  expect(bikelife.installSubpath).toBe(AUTO_SUBPATH);
  expect(routesByContent(bikelife)).toBe(true);
  expect(routesByContent(bikes)).toBe(false);
});

test("an installed Bikelife mod is looked for where it can land", () => {
  expect(scanSubpaths(bikelife)).toEqual(["mods/bikes", "mods/rider"]);
  expect(scanSubpaths(bikes)).toEqual(["mods/bikes"]);
  // The scans cover real folders only, each once: never `auto`.
  const paths = librarySubpaths("mxb");
  expect(paths).not.toContain(AUTO_SUBPATH);
  expect(new Set(paths).size).toBe(paths.length);
  expect(paths).toContain("mods/rider");
});

test("uninstall is told the folder the mod is really in", () => {
  expect(entrySubpath("C:\\MXB\\mods\\rider\\helmets\\Airoh\\paints\\Red.pnt", bikelife)).toBe("mods/rider");
  expect(entrySubpath("C:/MXB/mods/bikes/KTM 450/paints/x.pnt", bikelife)).toBe("mods/bikes");
  expect(entrySubpath("C:/MXB/mods/bikes/anything.pnt", bikes)).toBe("mods/bikes");
});

test("a store purchase is never filed as Bikelife", () => {
  // "Freeride / FMX / BikeLife" would match by name, and a purchase can't go through a review.
  const types = purchaseModTypes("mxb");
  expect(types.some(routesByContent)).toBe(false);
  expect(types.length).toBe(modTypesFor("mxb").length - 1);
  // GP Bikes has no Bikelife category at all.
  expect(modTypesFor("gpb").some(routesByContent)).toBe(false);
});
