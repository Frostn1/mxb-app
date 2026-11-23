import { describe, expect, test } from "bun:test";
import { JSDOM } from "jsdom";
import type { ModSummary } from "@frost/shared/types";
import { cleanSummaries } from "@frost/shared/api/mods";
import { appendPage } from "../src/lib/modPages";

const mod = (id: number): ModSummary => ({
  id,
  slug: `m-${id}`,
  title: `Mod ${id}`,
  link: "",
  date: "2026-11-08T02:20:38",
  image: null,
  categoryId: 22,
  author: null,
});
const page = (from: number, n = 24) => Array.from({ length: n }, (_, i) => mod(from + i));

describe("appendPage", () => {
  test("a fresh page appends and keeps paging", () => {
    const r = appendPage(page(1), page(25), 24);
    expect(r.mods.length).toBe(48);
    expect(r.hasMore).toBe(true);
  });

  test("a ranked page that repeats cards drops the repeats", () => {
    // Popular shifted between requests: page 2 starts with two of page 1's mods.
    const r = appendPage(page(1), [mod(23), mod(24), ...page(25, 22)], 24);
    expect(r.mods.map((m) => m.id)).toEqual(page(1, 46).map((m) => m.id));
    expect(new Set(r.mods.map((m) => m.id)).size).toBe(r.mods.length);
  });

  test("a page that adds nothing new ends the paging instead of looping", () => {
    // The fill-the-viewport effect asks for more while the grid is short. A full page of
    // repeats used to keep hasMore true forever: one request after another.
    const r = appendPage(page(1), page(1), 24);
    expect(r.mods.length).toBe(24);
    expect(r.hasMore).toBe(false);
  });

  test("a short page is the last one", () => {
    expect(appendPage([], page(1, 5), 24).hasMore).toBe(false);
  });
});

describe("cleanSummaries", () => {
  test("rows the grid can't key are dropped and missing fields become safe values", () => {
    const rows = cleanSummaries([
      mod(1),
      { id: 2, slug: "two" },
      { slug: "no-id", title: "x" },
      { id: "3", slug: "string-id" },
      null,
      "junk",
    ]);
    expect(rows.map((r) => r.id)).toEqual([1, 2]);
    expect(rows[1]).toEqual({
      id: 2,
      slug: "two",
      title: "two",
      link: "",
      date: "",
      image: null,
      categoryId: 0,
      author: null,
    });
  });

  test("an answer that isn't a list is an empty page, not a crash", () => {
    expect(cleanSummaries(null)).toEqual([]);
    expect(cleanSummaries({ items: [] })).toEqual([]);
  });
});

test("a listing that throws while rendering shows the error panel, not a blank window", async () => {
  const dom = new JSDOM("<!doctype html><div id=root></div>", { url: "http://localhost/" });
  const g = globalThis as Record<string, unknown>;
  const saved = ["window", "document", "navigator", "localStorage", "IS_REACT_ACT_ENVIRONMENT"].map(
    (k) => [k, g[k]] as const,
  );
  g.window = dom.window;
  g.document = dom.window.document;
  g.navigator = dom.window.navigator;
  g.localStorage = dom.window.localStorage;
  g.IS_REACT_ACT_ENVIRONMENT = true;
  const quiet = console.error;
  console.error = () => {};
  try {
    const { act } = await import("react");
    const { createRoot } = await import("react-dom/client");
    const { I18nProvider } = await import("../src/i18n");
    const { ListingBoundary } = await import("../src/Components/Browse/ListingError");
    function Broken(): never {
      throw new Error("a card had no title");
    }
    let retried = 0;
    const root = createRoot(dom.window.document.getElementById("root")!);
    await act(async () => {
      root.render(
        <I18nProvider>
          <ListingBoundary onRetry={() => retried++}>
            <Broken />
          </ListingBoundary>
        </I18nProvider>,
      );
    });
    const text = dom.window.document.body.textContent ?? "";
    expect(text).toContain("a card had no title");
    const retry = [...dom.window.document.querySelectorAll("button")].find((b) =>
      /retry/i.test(b.textContent ?? ""),
    );
    expect(retry).toBeDefined();
    expect(retried).toBe(0);
    await act(async () => root.unmount());
  } finally {
    console.error = quiet;
    for (const [k, v] of saved) g[k] = v;
  }
});
