import { describe, expect, it } from "vitest";
import { applyListingPage, upsertPost, writeMirrorVersion, type Category, type Listing } from "../src/mirror";
import { getAsset, searchAssets, syncedAt } from "../src/modapi";
import { d1 } from "./d1sqlite";
import { fakeBucket, fakeQueue } from "./modfakes";

const TREE = new Map<number, Category>([
  [22, { id: 22, name: "Tracks", parent: 118 }],
  [37, { id: 37, name: "Liveries", parent: 29 }],
  [128, { id: 128, name: "Enduro", parent: 318 }],
  [357, { id: 357, name: "Enduro", parent: 102 }],
]);

function env() {
  return { DB: d1(), ASSET_MIRROR: fakeBucket(), ASSET_LOCKED: fakeBucket(), MIRROR_QUEUE: fakeQueue(), MXB_ASSETS_CDN: "https://cdn.test" } as unknown as Env;
}

async function add(e: Env, id: number, slug: string, cat: number, published: string) {
  await upsertPost(e, { id, slug, link: `https://mxb-mods.com/${slug}/`, modified: published.slice(0, 19), date_gmt: published.slice(0, 19), categories: [cat], title: { rendered: slug } }, TREE, 1000);
  const a = (await e.DB.prepare("SELECT id FROM mod_assets WHERE source_ref = ?").bind(id).first<{ id: number }>())!;
  await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok' WHERE id = ?").bind(a.id).run();
  await writeMirrorVersion(e, a.id, null, [{ url: `https://x.example/${id}.zip`, host: "x", label: "x", isDefault: true, isServer: false }], 1000);
}

async function seed() {
  const e = env();
  await e.DB.prepare("INSERT INTO mirror_state (key, value) VALUES ('categories', ?)")
    .bind(JSON.stringify({ at: 1, cats: [...TREE.values()] }))
    .run();
  await add(e, 101, "old-track", 22, "2026-01-01T00:00:00Z");
  await add(e, 102, "mid-livery", 37, "2026-02-01T00:00:00Z");
  await add(e, 103, "new-track", 22, "2026-03-01T00:00:00Z");
  return e;
}

const search = async (e: Env, q: string) => (await searchAssets(new URL(`https://x/v1/assets/search${q}`), e));
type Body = { synced_at: string | null; results: { wp_id: number; wp_slug: string; source_url: string }[] };

describe("WordPress-compatible catalogue", () => {
  it("carries the source post's id and slug", async () => {
    const e = await seed();
    const r = await search(e, "?category=22");
    expect(r.status).toBe(200);
    const b = r.body as Body;
    expect(b.results.map((x) => [x.wp_id, x.wp_slug])).toEqual([[103, "new-track"], [101, "old-track"]]);
    expect(b.results[0].source_url).toBe("https://mxb-mods.com/new-track/");
  });

  it("filters by category id, newest first, and oldest first on request", async () => {
    const e = await seed();
    expect(((await search(e, "?category=37")).body as Body).results.map((x) => x.wp_id)).toEqual([102]);
    expect(((await search(e, "?category=22&sort=oldest")).body as Body).results.map((x) => x.wp_id)).toEqual([101, 103]);
  });

  it("refuses a category it cannot match exactly", async () => {
    const e = await seed();
    expect(await search(e, "?category=999")).toEqual({ status: 400, body: { error: "unknown category" } });
    // Two categories called Enduro: a name match would answer for both.
    expect(await search(e, "?category=128")).toEqual({ status: 400, body: { error: "unknown category" } });
    expect(await search(e, "?category=abc")).toEqual({ status: 400, body: { error: "unknown category" } });
  });

  it("reports when the listing walk last finished", async () => {
    const e = await seed();
    expect(await syncedAt(e)).toBeNull();
    expect(((await search(e, "")).body as Body).synced_at).toBeNull();
    const st: Listing = { hwm: "", walk: null };
    // A short page is the end of the walk.
    expect(await applyListingPage(e, st, [], TREE, 1_800_000_000_000)).toBe("done");
    expect(await syncedAt(e)).toBe(new Date(1_800_000_000_000).toISOString());
    expect(((await search(e, "")).body as Body).synced_at).toBe(new Date(1_800_000_000_000).toISOString());
    const id = (await e.DB.prepare("SELECT id FROM mod_assets WHERE source_ref = 101").first<{ id: number }>())!.id;
    const one = await getAsset(id, new URL("https://x/v1/assets/x"), e);
    expect((one.body as { synced_at: string; wp_id: number }).wp_id).toBe(101);
    expect((one.body as { synced_at: string }).synced_at).toBe(new Date(1_800_000_000_000).toISOString());
  });
});
