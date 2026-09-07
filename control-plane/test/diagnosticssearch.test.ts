import { describe, expect, it } from "vitest";
import {
  clampDays,
  claimText,
  likeTerm,
  oneOf,
  parseFileQuery,
  parsePage,
  parseRiderQuery,
  parseSightingQuery,
} from "../src/diagnosticssearch";

const at = (query: string) => new URL(`https://example.com/admin/diagnostics${query}`);

describe("turning a search box into a LIKE pattern", () => {
  it("wraps what was typed", () => {
    expect(likeTerm("overlay")).toBe("%overlay%");
  });

  it("escapes the wildcards, so a typed underscore is an underscore", () => {
    expect(likeTerm("nvidia_share")).toBe("%nvidia\\_share%");
    expect(likeTerm("50%")).toBe("%50\\%%");
    expect(likeTerm("a\\b")).toBe("%a\\\\b%");
  });

  it("is empty for an empty box, which every caller reads as no filter", () => {
    expect(likeTerm("")).toBe("");
    expect(likeTerm("   ")).toBe("");
  });

  it("bounds what one box can send to the database", () => {
    expect(likeTerm("x".repeat(400))).toBe(`%${"x".repeat(96)}%`);
  });
});

describe("paging", () => {
  it("starts at one", () => {
    expect(parsePage(null)).toBe(1);
    expect(parsePage("1")).toBe(1);
  });

  it("refuses a page nobody could have clicked to", () => {
    expect(parsePage("0")).toBe(1);
    expect(parsePage("-4")).toBe(1);
    expect(parsePage("nonsense")).toBe(1);
    expect(parsePage("1e9")).toBe(10_000);
  });

  it("takes a page in the middle", () => {
    expect(parsePage("7")).toBe(7);
    expect(parsePage("7.9")).toBe(7);
  });
});

describe("the day window", () => {
  it("defaults to a month and clamps to a year", () => {
    expect(clampDays(null)).toBe(30);
    expect(clampDays("400")).toBe(365);
    expect(clampDays("0")).toBe(1);
    expect(clampDays("rubbish")).toBe(30);
  });
});

describe("keeping a query string out of the SQL", () => {
  it("takes one of the set", () => {
    expect(oneOf("warn", ["any", "warn", "alert"] as const)).toBe("warn");
  });

  it("falls back to the first for anything else", () => {
    expect(oneOf("'; DROP TABLE accounts--", ["any", "warn"] as const)).toBe("any");
    expect(oneOf(null, ["any", "warn"] as const)).toBe("any");
  });
});

describe("what a file says it is", () => {
  it("prefers the description, and names the company beside it", () => {
    expect(claimText("Steam overlay", "Steam", "Valve Corporation")).toBe(
      "Steam overlay — Valve Corporation",
    );
  });

  it("does not repeat the company when it is the only thing said", () => {
    expect(claimText("", "", "NVIDIA Corporation")).toBe("NVIDIA Corporation");
  });

  it("is empty when the file claims nothing, which is itself worth seeing", () => {
    expect(claimText("", "", "")).toBe("");
  });
});

describe("reading a rider search off the URL", () => {
  it("defaults to everyone, most recently reporting first", () => {
    expect(parseRiderQuery(at("/riders"))).toEqual({
      q: "",
      state: "any",
      sort: "seen",
      days: 30,
      page: 1,
    });
  });

  it("takes what was asked for", () => {
    const q = parseRiderQuery(at("/riders?q=Frost&state=alert&sort=name&days=7&page=3"));
    expect(q).toEqual({ q: "Frost", state: "alert", sort: "name", days: 7, page: 3 });
  });

  it("drops a state and a sort it does not have", () => {
    const q = parseRiderQuery(at("/riders?state=banned&sort=guid"));
    expect(q.state).toBe("any");
    expect(q.sort).toBe("seen");
  });
});

describe("reading a file search off the URL", () => {
  it("opens on what nothing accounts for", () => {
    expect(parseFileQuery(at("/files"))).toEqual({
      q: "",
      state: "flagged",
      trust: "any",
      origin: "any",
      sort: "state",
      days: 30,
      page: 1,
    });
  });

  it("takes every filter together", () => {
    const q = parseFileQuery(
      at("/files?q=overlay&state=any&trust=unsigned&origin=other&sort=accounts&days=90&page=2"),
    );
    expect(q).toEqual({
      q: "overlay",
      state: "any",
      trust: "unsigned",
      origin: "other",
      sort: "accounts",
      days: 90,
      page: 2,
    });
  });

  it("drops a trust and an origin it does not have", () => {
    const q = parseFileQuery(at("/files?trust=probably&origin=steam"));
    expect(q.trust).toBe("any");
    expect(q.origin).toBe("any");
  });
});

describe("reading a rider's own file filter off the URL", () => {
  it("uses its own names, so it does not collide with the rider search", () => {
    const q = parseSightingQuery(at("/rider?id=acc1&f=d3d9&fstate=warn&page=2"));
    expect(q).toEqual({ q: "d3d9", state: "warn", page: 2 });
  });

  it("defaults to every file that rider has", () => {
    expect(parseSightingQuery(at("/rider?id=acc1"))).toEqual({ q: "", state: "any", page: 1 });
  });
});
