import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { discoveryResult } from "../src/mirror";
import { d1 } from "./d1sqlite";

const NOW = Date.UTC(2026, 9, 8);
const LISTING = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "fixtures", "mods", "wp-listing-offset2025.json"), "utf8");

// A real page from mxb-mods.com, as the home fetcher hands it in. Entry 10 (post 167620, whose
// author was deleted) comes back as `_links` and `_embedded` only: no id, slug, link or date.
// That one husk used to throw in D1 and answer the whole result 500, so discovery never moved.
describe("fetcher listing result", () => {
  it("takes a real 50-post WordPress listing with a husk in it", async () => {
    const e = { DB: d1() } as unknown as Env;
    const set = (k: string, v: unknown) => e.DB.prepare("INSERT INTO mirror_state (key, value) VALUES (?, ?)").bind(k, JSON.stringify(v)).run();
    await set("categories", { at: NOW, cats: [{ id: 1, name: "Bikes", parent: 0 }] });
    await set("listing", { hwm: "", walk: { top: "", offset: 2025 } });
    await set("fetcher_discovery", { seq: 1, phase: "listing", catPage: 1, cats: [], listed: 0, roundAt: NOW, leasedUntil: NOW + 60_000, nextAt: 0 });
    expect(await discoveryResult(e, 1, { status: 200, body: LISTING }, NOW)).toBe(true);
    const n = await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_assets WHERE source = 'mirror'").first<{ n: number }>();
    expect(n?.n).toBe(49);
    const listing = await e.DB.prepare("SELECT value FROM mirror_state WHERE key = 'listing'").first<{ value: string }>();
    // The walk moves by the page's length, husk included.
    expect(JSON.parse(listing!.value).walk.offset).toBe(2025 + 45);
  });
});
