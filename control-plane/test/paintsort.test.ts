/**
 * How a paint table is ordered.
 *
 * Two things are being pinned here. The first is that a column name arriving in a query
 * string can only ever select one of the fixed expressions in the map — this is the only
 * place in the module where anything typed by a caller decides part of a SQL statement, and
 * it must decide it by lookup rather than by interpolation. The second is that every listing
 * has a tie-break, without which two pages of the same query can show the same row twice.
 */

import { describe, expect, it } from "vitest";
import {
  orderBy,
  PAINT_COLUMNS,
  parseDir,
  parseOrder,
  RIDER_COLUMNS,
  type Order,
} from "../src/paintspage";

const at = (query: string) => new URL(`https://example.com/admin/paints${query}`);

describe("which column was asked for", () => {
  it("takes one the table has", () => {
    expect(parseOrder(at("?sort=size"), RIDER_COLUMNS, "published").sort).toBe("size");
  });

  it("falls back to the column the table opens on", () => {
    expect(parseOrder(at(""), RIDER_COLUMNS, "published").sort).toBe("published");
    expect(parseOrder(at("?sort=nonsense"), RIDER_COLUMNS, "published").sort).toBe("published");
  });

  it("does not take a column from the other table", () => {
    expect(parseOrder(at("?sort=uses"), RIDER_COLUMNS, "published").sort).toBe("published");
    expect(parseOrder(at("?sort=steam"), PAINT_COLUMNS, "riders").sort).toBe("riders");
  });

  it("refuses anything that is not one of the two directions", () => {
    expect(parseDir("asc", "desc")).toBe("asc");
    expect(parseDir("DESC; DROP TABLE accounts", "asc")).toBe("asc");
    expect(parseDir(null, "desc")).toBe("desc");
  });

  it("opens each column the way that column is worth reading", () => {
    // A name is read from the top, a quantity or a date from the largest or the newest.
    expect(parseOrder(at("?sort=name"), RIDER_COLUMNS, "published").dir).toBe("asc");
    expect(parseOrder(at("?sort=size"), RIDER_COLUMNS, "published").dir).toBe("desc");
    expect(parseOrder(at("?sort=file"), PAINT_COLUMNS, "riders").dir).toBe("asc");
    expect(parseOrder(at("?sort=riders"), PAINT_COLUMNS, "riders").dir).toBe("desc");
  });

  it("still takes the direction that was asked for", () => {
    expect(parseOrder(at("?sort=name&dir=desc"), RIDER_COLUMNS, "published").dir).toBe("desc");
  });
});

describe("the ORDER BY it builds", () => {
  const order = (sort: string, dir: "asc" | "desc"): Order => ({ sort, dir });

  it("names the expression the column stands for, not the alias it is selected under", () => {
    // `size` is also a column of `loadout_paints`; ordering by the bare alias is one rename
    // away from sorting by a single row's size rather than the group's.
    expect(orderBy(PAINT_COLUMNS, order("size", "desc"), "x")).toContain("MAX(p.size) DESC");
    expect(orderBy(RIDER_COLUMNS, order("size", "desc"), "x")).toContain("SUM(p.size) DESC");
  });

  it("keeps the empties at the bottom whichever way a text column is read", () => {
    const up = orderBy(RIDER_COLUMNS, order("guid", "asc"), "x");
    const down = orderBy(RIDER_COLUMNS, order("guid", "desc"), "x");
    expect(up).toContain("(a.guid IS NULL OR a.guid = '')");
    expect(down).toContain("(a.guid IS NULL OR a.guid = '')");
    expect(up).toContain("COLLATE NOCASE ASC");
    expect(down).toContain("COLLATE NOCASE DESC");
  });

  it("keeps a rider who has never published under one who has", () => {
    expect(orderBy(RIDER_COLUMNS, order("published", "desc"), "x")).toBe(
      "published_at IS NULL, published_at DESC, x",
    );
  });

  it("always ends in the tie-break, so paging cannot repeat a row", () => {
    for (const [columns, fallback] of [
      [RIDER_COLUMNS, "published"],
      [PAINT_COLUMNS, "riders"],
    ] as const) {
      for (const key of Object.keys(columns)) {
        for (const dir of ["asc", "desc"] as const) {
          expect(orderBy(columns, order(key, dir), "tie")).toMatch(/, tie$/);
        }
      }
      expect(orderBy(columns, order(fallback, "desc"), "tie")).toMatch(/, tie$/);
    }
  });

  it("emits nothing but the two words for a direction", () => {
    for (const columns of [RIDER_COLUMNS, PAINT_COLUMNS]) {
      for (const key of Object.keys(columns)) {
        for (const dir of ["asc", "desc"] as const) {
          const sql = orderBy(columns, order(key, dir), "tie");
          expect(sql).toContain(dir.toUpperCase());
          expect(sql).not.toMatch(/;|--/);
        }
      }
    }
  });
});
