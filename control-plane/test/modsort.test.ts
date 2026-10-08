import { describe, expect, it } from "vitest";
import { upsertPost, writeMirrorVersion, type Category } from "../src/mirror";
import { searchAssets } from "../src/modapi";
import { d1 } from "./d1sqlite";
import { fakeBucket, fakeQueue } from "./modfakes";

const TREE = new Map<number, Category>([[22, { id: 22, name: "Tracks", parent: 118 }]]);

function env() {
  return { DB: d1(), ASSET_MIRROR: fakeBucket(), ASSET_LOCKED: fakeBucket(), MIRROR_QUEUE: fakeQueue(), MXB_ASSETS_CDN: "https://cdn.test" } as unknown as Env;
}

interface Spec {
  id: number;
  title: string;
  published: string;
  modified: string;
  size: number;
}

/** A listed mirrored mod with the given dates and stored size. */
async function add(e: Env, m: Spec) {
  await upsertPost(e, { id: m.id, slug: `m${m.id}`, link: `https://mxb-mods.com/m${m.id}/`, modified: m.modified, categories: [22], title: { rendered: m.title } }, TREE, 1000);
  const a = (await e.DB.prepare("SELECT id FROM mod_assets WHERE source_ref = ?").bind(m.id).first<{ id: number }>())!;
  await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok', published = ? WHERE id = ?").bind(m.published, a.id).run();
  const v = await writeMirrorVersion(e, a.id, null, [{ url: `https://x.example/${m.id}.zip`, host: "x", label: "x", isDefault: true, isServer: false }], 1000);
  const sha = String(m.id).padStart(64, "0");
  await e.DB.prepare("INSERT INTO mod_blobs (sha256, bucket, r2_key, size, first_seen) VALUES (?, 'public', ?, ?, 0)").bind(sha, `k${m.id}`, m.size).run();
  await e.DB.prepare("UPDATE mod_files SET status = 'done', sha256 = ? WHERE version_id = ?").bind(sha, v).run();
}

const MODS: Spec[] = [
  { id: 1, title: "bravo", published: "2026-03-01T00:00:00Z", modified: "2026-09-01T00:00:00", size: 500 },
  { id: 2, title: "Alpha", published: "2026-05-01T00:00:00Z", modified: "2026-04-01T00:00:00", size: 100 },
  { id: 3, title: "charlie", published: "2026-01-01T00:00:00Z", modified: "2026-06-01T00:00:00", size: 900 },
  // Same date, name and size as 2: the id decides.
  { id: 4, title: "alpha", published: "2026-05-01T00:00:00Z", modified: "2026-04-01T00:00:00", size: 100 },
];

async function titles(e: Env, query: string) {
  const r = await searchAssets(new URL(`https://x/v1/assets/search${query}`), e);
  expect(r.status).toBe(200);
  return (r.body as { results: { title: string; id: string }[] }).results.map((x) => x.title);
}

describe("catalogue sort", () => {
  it("orders by each option, ties broken by id", async () => {
    const e = env();
    for (const m of MODS) await add(e, m);
    // Ids 4 then 2 for equal keys when descending; 2 then 4 when ascending.
    expect(await titles(e, "")).toEqual(["alpha", "Alpha", "bravo", "charlie"]);
    expect(await titles(e, "?sort=newest")).toEqual(["alpha", "Alpha", "bravo", "charlie"]);
    expect(await titles(e, "?sort=updated")).toEqual(["bravo", "charlie", "alpha", "Alpha"]);
    expect(await titles(e, "?sort=name")).toEqual(["Alpha", "alpha", "bravo", "charlie"]);
    expect(await titles(e, "?sort=size")).toEqual(["charlie", "bravo", "alpha", "Alpha"]);
  });

  it("keeps the type filter and a search, and pages without overlap", async () => {
    const e = env();
    for (const m of MODS) await add(e, m);
    expect(await titles(e, "?sort=size&type=tracks")).toEqual(["charlie", "bravo", "alpha", "Alpha"]);
    expect(await titles(e, "?sort=size&type=bikes")).toEqual([]);
    expect(await titles(e, "?sort=name&q=charlie")).toEqual(["charlie"]);
    // With a search and no sort named, relevance leads; the listed set is the same.
    expect((await titles(e, "?q=alpha")).sort()).toEqual(["Alpha", "alpha"]);
  });

  it("refuses an unknown sort", async () => {
    const r = await searchAssets(new URL("https://x/v1/assets/search?sort=drop"), env());
    expect(r).toEqual({ status: 400, body: { error: "unknown sort" } });
  });
});
