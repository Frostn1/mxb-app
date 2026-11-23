import { describe, expect, it } from "bun:test";
import { MOD_TYPES } from "@frost/shared/api/mods";
import real from "./fixtures/mxbmods-categories.json";

// GET /v1/mxbmods/categories as mxb-mods answered on 2026-11-12.
const byId = new Map(real.items.map((c) => [c.id, c]));

describe("mxb-mods browse categories", () => {
  it("every filter the app offers is a category the site still has", () => {
    for (const mt of MOD_TYPES) {
      for (const c of mt.categories) {
        expect([mt.id, c.id, byId.has(c.id)]).toEqual([mt.id, c.id, true]);
      }
    }
  });

  it("each filter sits under its type's root category (or is the root)", () => {
    for (const mt of MOD_TYPES) {
      for (const c of mt.categories) {
        let id = c.id;
        const seen = new Set<number>();
        while (id !== mt.categoryId && byId.get(id)?.parent && !seen.has(id)) {
          seen.add(id);
          id = byId.get(id)!.parent;
        }
        // Track Assets lives under Other (40) on the site but is offered beside the tracks.
        const ok = id === mt.categoryId || (mt.id === "tracks" && c.id === 119);
        expect([mt.id, c.id, ok]).toEqual([mt.id, c.id, true]);
      }
    }
  });
});
