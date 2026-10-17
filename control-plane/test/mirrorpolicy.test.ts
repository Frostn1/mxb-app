import { describe, expect, it } from "vitest";
import { upsertPost, writeMirrorVersion, type Category } from "../src/mirror";
import { mirrorFile } from "../src/mirrorfetch";
import { activeBikes, bikeKey, evictUnused, modelTerms, supported, wantLiveTracks } from "../src/mirrorpolicy";
import { getAsset, publicModRoutes, searchAssets } from "../src/modapi";
import { checkLinks, RECHECK_MS } from "../src/linkhealth";
import { addAccount, d1 } from "./d1sqlite";
import { fakeBucket, fakeFetch, fakeQueue, nodeHasher } from "./modfakes";

const DAY = 24 * 3600_000;
const NOW = 400 * DAY;

const TREE = new Map<number, Category>(
  [
    { id: 22, name: "Tracks", parent: 118 },
    { id: 29, name: "Bikes", parent: 118 },
    { id: 37, name: "Liveries", parent: 29 },
    { id: 35, name: "Rider Kit", parent: 30 },
    { id: 319, name: "Manufacturers", parent: 0 },
    { id: 102, name: "KTM", parent: 319 },
    { id: 147, name: "Discontinued", parent: 0 },
    { id: 108, name: "2021 KTM 450 SX-F OEM", parent: 147 },
    { id: 900, name: "2023 KTM 450 SX-F OEM", parent: 147 },
  ].map((c) => [c.id, c]),
);

function env(extra: Record<string, unknown> = {}) {
  return {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue(),
    MXB_ASSETS_CDN: "https://cdn.mxbsecure.com",
    ...extra,
  } as unknown as Env & { MIRROR_QUEUE: ReturnType<typeof fakeQueue>; ASSET_MIRROR: ReturnType<typeof fakeBucket> };
}

/** A mirrored post whose page has been read, offering one link. */
async function post(e: Env, id: number, slug: string, cats: number[], url = `https://x.example/${slug}.zip`) {
  await upsertPost(
    e,
    { id, slug, link: `https://mxb-mods.com/${slug}/`, modified: "2026-01-01T00:00:00", categories: cats, title: { rendered: slug } },
    TREE,
    NOW - 10 * DAY,
  );
  const a = await e.DB.prepare("SELECT id, public_id FROM mod_assets WHERE source_ref = ?").bind(id).first<{ id: number; public_id: string }>();
  await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok' WHERE id = ?").bind(a!.id).run();
  const v = await writeMirrorVersion(e, a!.id, null, [{ url, host: "x", label: "x", isDefault: true, isServer: false }], NOW);
  return { asset: a!.id, pub: a!.public_id, version: v };
}

async function ride(e: Env, bikeId: string, at = NOW - DAY) {
  await addAccount(e.DB, `acc-${bikeId}`, `r-${bikeId}`);
  await e.DB.prepare("INSERT INTO loadouts (account_id, bike_id, updated_at) VALUES (?, ?, ?)").bind(`acc-${bikeId}`, bikeId, at).run();
}

const status = async (e: Env, version: number) =>
  (await e.DB.prepare("SELECT status FROM mod_files WHERE version_id = ? AND part = 0").bind(version).first<{ status: string }>())!.status;

describe("the supported-bike rule", () => {
  it("spells a category and a game folder the same way", () => {
    expect(bikeKey("2023 KTM 450 SX-F OEM")).toBe("2023ktm450sxf");
    expect(bikeKey("MX1OEM_2023_KTM_450_SX-F")).toBe("2023ktm450sxf");
    expect(bikeKey("MXEOEM_2023_Stark_VARG")).toBe(bikeKey("2023 Stark VARG OEM"));
    expect(modelTerms("KTM; 2021 KTM 450 SX-F OEM")).toEqual(["2021 KTM 450 SX-F OEM"]);
  });

  it("keeps liveries for bikes riders ride, skips those for bikes nobody does", async () => {
    const e = env();
    await ride(e, "MX1OEM_2023_KTM_450_SX-F");
    await ride(e, "MX1OEM_2021_KTM_450_SX-F", NOW - 400 * DAY); // ridden, but long ago
    const bikes = await activeBikes(e, NOW);
    expect(supported({ type: "liveries", bike: "KTM; 2023 KTM 450 SX-F OEM" }, bikes)).toMatchObject({ ok: true });
    expect(supported({ type: "liveries", bike: "KTM; 2021 KTM 450 SX-F OEM" }, bikes)).toEqual({
      ok: false,
      reason: "no rider has used this bike recently",
    });
    expect(supported({ type: "liveries", bike: "KTM" }, bikes)).toMatchObject({ ok: true, reason: "no bike model named" });
    expect(supported({ type: "kits", bike: "" }, bikes)).toMatchObject({ ok: true });
  });

  it("lets Sean's lists override it either way", async () => {
    const e = env({ MXB_MIRROR_BIKES_ALLOW: "2021 KTM 450 SX-F OEM", MXB_MIRROR_BIKES_DENY: "MX1OEM_2023_KTM_450_SX-F" });
    await ride(e, "MX1OEM_2023_KTM_450_SX-F");
    const bikes = await activeBikes(e, NOW);
    expect(supported({ type: "liveries", bike: "2021 KTM 450 SX-F OEM" }, bikes).ok).toBe(true);
    expect(supported({ type: "liveries", bike: "2023 KTM 450 SX-F OEM" }, bikes).ok).toBe(false);
  });
});

describe("mirror on demand", () => {
  const dl = (e: Env, asset: string) => {
    const u = `https://api.mxbsecure.com/v1/assets/${asset}/download/0`;
    return publicModRoutes(new Request(u), new URL(u), e);
  };

  it("lists the whole catalogue, unmirrored, with the original link", async () => {
    const e = env();
    const { asset, pub } = await post(e, 1, "red-bull-ktm", [37, 102, 900]);
    const s = (await searchAssets(new URL("https://x/v1/assets/search?q=red"), e)).body as { total: number };
    expect(s.total).toBe(1);
    const d = (await getAsset(asset, new URL("https://api/x"), e)).body as { files: { state: string; download: string; source: string }[] };
    expect(d.files[0]).toMatchObject({ state: "original", source: "https://x.example/red-bull-ktm.zip" });
    expect(d.files[0].download).toContain(`/v1/assets/${pub}/download/0`);
  });

  it("a first download goes to the original and queues a copy; the next one comes from the CDN", async () => {
    const e = env();
    // The route reads the clock itself, so this rider is "recent" by the real one.
    await ride(e, "MX1OEM_2023_KTM_450_SX-F", Date.now());
    const { pub: asset, version } = await post(e, 1, "red-bull-ktm", [37, 102, 900]);
    const first = await dl(e, asset);
    expect(first!.status).toBe(302);
    expect(first!.headers.get("location")).toBe("https://x.example/red-bull-ktm.zip");
    expect(e.MIRROR_QUEUE.sent).toEqual([{ kind: "file", version, idx: 0, part: 0 }]);

    // A second click before the copy lands doesn't queue it twice.
    await dl(e, asset);
    expect(e.MIRROR_QUEUE.sent).toHaveLength(1);

    const f = fakeFetch([[/x\.example/, () => new Response("PNT\0", { headers: { "content-type": "application/octet-stream", "content-length": "4" } })]]);
    await mirrorFile(e, { kind: "file", version, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher, now: NOW });
    const second = await dl(e, asset);
    expect(second!.headers.get("location")).toMatch(/^https:\/\/cdn\.mxbsecure\.com\/liveries\/[0-9a-f]{64}$/);
  });

  it("never copies a livery for a bike nobody rides; the download still works", async () => {
    const e = env();
    const { pub: asset, version } = await post(e, 2, "old-ktm", [37, 102, 108]);
    const r = await dl(e, asset);
    expect(r!.headers.get("location")).toBe("https://x.example/old-ktm.zip");
    expect(e.MIRROR_QUEUE.sent).toEqual([]);
    expect(await status(e, version)).toBe("idle");
  });
});

describe("prepare: the Download button's wait", () => {
  const prep = (e: Env, asset: string, path = "0") => {
    const u = `https://api.mxbsecure.com/v1/assets/${asset}/prepare/${path}`;
    return publicModRoutes(new Request(u, { method: "POST" }), new URL(u), e);
  };
  const body = async (r: Response | null) => (await r!.json()) as { state: string; download?: string; source?: string | null };

  it("queues the copy once, says mirroring, then hands over our download", async () => {
    const e = env();
    const { pub, version } = await post(e, 1, "hills-mx", [22]);
    const first = await prep(e, pub);
    expect(first!.headers.get("cache-control")).toBe("no-store");
    expect(await body(first)).toEqual({ state: "mirroring" });
    expect(await body(await prep(e, pub))).toEqual({ state: "mirroring" });
    expect(e.MIRROR_QUEUE.sent).toEqual([{ kind: "file", version, idx: 0, part: 0 }]);

    const f = fakeFetch([[/x\.example/, () => new Response("PK", { headers: { "content-length": "4" } })]]);
    await mirrorFile(e, { kind: "file", version, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher, now: NOW });
    const done = await body(await prep(e, pub));
    expect(done).toEqual({ state: "stored", download: `https://api.mxbsecure.com/v1/assets/${pub}/download/0?version=1` });
  });

  it("falls back to the original when the host refuses, or the policy won't keep it", async () => {
    const e = env();
    const { pub, version } = await post(e, 1, "quota-mx", [22]);
    await e.DB.prepare("UPDATE mod_files SET status = 'retry', error = 'quota' WHERE version_id = ?").bind(version).run();
    expect(await body(await prep(e, pub))).toEqual({ state: "original", source: "https://x.example/quota-mx.zip" });

    const old = await post(e, 2, "old-ktm", [37, 102, 108]);
    expect(await body(await prep(e, old.pub))).toEqual({ state: "original", source: "https://x.example/old-ktm.zip" });
    expect(e.MIRROR_QUEUE.sent).toEqual([]);
  });

  it("marks a link the host answered 404 as broken, in prepare and on the mod", async () => {
    const e = env();
    const { asset, pub, version } = await post(e, 1, "gone-mx", [22]);
    await e.DB.prepare("UPDATE mod_files SET status = 'failed', error = 'gdrive: the file is gone (404)' WHERE version_id = ?")
      .bind(version)
      .run();
    expect(await body(await prep(e, pub))).toEqual({ state: "original", source: "https://x.example/gone-mx.zip", broken: true });
    const res = await getAsset(asset, new URL("https://api/x"), e);
    expect((res.body as { files: { broken: boolean }[] }).files[0].broken).toBe(true);

    await e.DB.prepare("UPDATE mod_files SET status = 'failed', error = 'larger than 2147483648 bytes' WHERE version_id = ?")
      .bind(version)
      .run();
    expect(await body(await prep(e, pub))).toEqual({ state: "original", source: "https://x.example/gone-mx.zip" });
  });

  it("answers 404 for a file the mod doesn't have", async () => {
    const e = env();
    const { pub } = await post(e, 1, "hills-mx", [22]);
    expect((await prep(e, pub, "3"))!.status).toBe(404);
  });
});

describe("a mirrored mod by its source slug", () => {
  it("finds the post the app browses, with its files", async () => {
    const e = env();
    const { pub } = await post(e, 7, "hillsford-mx-park", [22]);
    const u = "https://api.mxbsecure.com/v1/assets/mirror/hillsford-mx-park";
    const r = await publicModRoutes(new Request(u), new URL(u), e);
    expect(r!.status).toBe(200);
    const d = (await r!.json()) as { id: string; files: { source: string }[] };
    expect(d.id).toBe(pub);
    expect(d.files[0].source).toBe("https://x.example/hillsford-mx-park.zip");
    const miss = "https://api.mxbsecure.com/v1/assets/mirror/nope";
    expect((await publicModRoutes(new Request(miss), new URL(miss), e))!.status).toBe(404);
  });
});

describe("live-server tracks", () => {
  async function seen(e: Env, slug: string, at: number, exact = 1) {
    await e.DB.prepare(
      "INSERT INTO track_catalog (track_key, track_id, source, exact, name, url, slug, requested_at) VALUES (?, ?, 'mods', ?, ?, ?, ?, ?)",
    )
      .bind(slug.replace(/-/g, ""), slug, exact, slug, `https://mxb-mods.com/${slug}/`, slug, at)
      .run();
  }

  it("pre-mirrors tracks the Servers tab saw on a server recently, and only those", async () => {
    const e = env();
    const live = await post(e, 10, "farm-14", [22]);
    const stale = await post(e, 11, "old-mx", [22]);
    const guess = await post(e, 12, "forest", [22]);
    const unseen = await post(e, 13, "nobody-hosts", [22]);
    await seen(e, "farm-14", NOW - 2 * DAY);
    await seen(e, "old-mx", NOW - 30 * DAY);
    await seen(e, "forest", NOW - DAY, 0); // a resemblance, not the track itself
    expect(await wantLiveTracks(e, NOW)).toBe(1);
    expect(await status(e, live.version)).toBe("pending");
    for (const v of [stale, guess, unseen]) expect(await status(e, v.version)).toBe("idle");
  });

  it("evicts what nobody downloaded, keeps live tracks and uploads", async () => {
    const e = env({ MXB_MIRROR_RETAIN_DAYS: "90" });
    const live = await post(e, 10, "farm-14", [22]);
    const cold = await post(e, 11, "red-bull", [37]);
    await seen(e, "farm-14", NOW - DAY);
    const put = async (sha: string, version: number) => {
      await e.ASSET_MIRROR.put(`x/${sha}`, "data");
      await e.DB.prepare("INSERT INTO mod_blobs (sha256, bucket, r2_key, size, first_seen, last_used_at) VALUES (?, 'public', ?, 4, ?, ?)")
        .bind(sha, `x/${sha}`, NOW - 200 * DAY, NOW - 100 * DAY)
        .run();
      await e.DB.prepare("UPDATE mod_files SET status = 'done', sha256 = ? WHERE version_id = ?").bind(sha, version).run();
    };
    await put("a".repeat(64), live.version);
    await put("b".repeat(64), cold.version);
    expect(await evictUnused(e, NOW)).toBe(1);
    expect([...e.ASSET_MIRROR.objects.keys()]).toEqual([`x/${"a".repeat(64)}`]);
    expect(await status(e, cold.version)).toBe("idle");
    expect(await status(e, live.version)).toBe("done");
  });
});

describe("link health", () => {
  const files = async (e: Env, asset: number) =>
    ((await getAsset(asset, new URL("https://api/x"), e)).body as { files: { link: string | null; broken: boolean }[] }).files;

  it("checks unstored links gently, marks a 404 broken and a live one ok", async () => {
    const e = env();
    const dead = await post(e, 1, "dead-mx", [22], "https://x.example/dead.zip");
    const live = await post(e, 2, "live-mx", [22], "https://y.example/live.zip");
    const mega = await post(e, 3, "mega-mx", [22], "https://mega.nz/file/abc#key");
    const f = fakeFetch([
      [/x\.example/, () => new Response(null, { status: 404 })],
      [/y\.example/, () => new Response(null, { status: 200 })],
    ]);
    const sleeps: number[] = [];
    const r = await checkLinks(e, { fetch: f, now: NOW, sleep: async (ms) => void sleeps.push(ms) });
    expect(r).toEqual({ checked: 2, broken: 1 });
    expect(f.calls).toEqual(["HEAD https://x.example/dead.zip", "HEAD https://y.example/live.zip"]);
    expect(sleeps).toEqual([1000]);
    expect((await files(e, dead.asset))[0]).toMatchObject({ link: "broken", broken: true });
    expect((await files(e, live.asset))[0]).toMatchObject({ link: "ok", broken: false });
    expect((await files(e, mega.asset))[0]).toMatchObject({ link: null, broken: false });

    // Nothing is due again until the recheck interval has passed.
    expect(await checkLinks(e, { fetch: f, now: NOW + 1000, sleep: async () => {} })).toEqual({ checked: 0, broken: 0 });
    expect((await checkLinks(e, { fetch: f, now: NOW + RECHECK_MS + 1, sleep: async () => {} })).checked).toBe(2);
  });

  it("says nothing on a refusal or a rate limit, and revives a link found alive again", async () => {
    const e = env();
    const { asset, version } = await post(e, 1, "back-mx", [22], "https://x.example/back.zip");
    await e.DB.prepare("UPDATE mod_files SET status = 'failed', error = 'direct: the file is gone (404)' WHERE version_id = ?").bind(version).run();
    expect((await files(e, asset))[0]).toMatchObject({ link: "broken", broken: true });
    await checkLinks(e, { fetch: fakeFetch([[/x\.example/, () => new Response(null, { status: 429 })]]), now: NOW, sleep: async () => {} });
    expect((await files(e, asset))[0]).toMatchObject({ link: "broken" });
    await checkLinks(e, { fetch: fakeFetch([[/x\.example/, () => new Response(null, { status: 200 })]]), now: NOW + RECHECK_MS + 1, sleep: async () => {} });
    expect((await files(e, asset))[0]).toMatchObject({ link: "ok", broken: false });
    expect(await status(e, version)).toBe("idle");
  });

  it("reads Drive's own answers: a 404 and a sign-in bounce are broken", async () => {
    const e = env();
    const gone = await post(e, 1, "drive-gone", [22], "https://drive.google.com/file/d/GONE/view?usp=sharing");
    const priv = await post(e, 2, "drive-private", [22], "https://drive.google.com/file/d/PRIV/view?usp=sharing");
    const ok = await post(e, 3, "drive-ok", [22], "https://drive.google.com/file/d/OKAY/view?usp=sharing");
    const f = fakeFetch([
      [/GONE/, () => new Response("<title>Google Drive - Page Not Found</title>", { status: 404 })],
      [/PRIV/, () => Object.defineProperty(new Response("<title>Sign in</title>"), "url", { value: "https://accounts.google.com/ServiceLogin?x" })],
      [/OKAY/, () => new Response("<title>E10.zip - Google Drive</title>")],
    ]);
    await checkLinks(e, { fetch: f, now: NOW, sleep: async () => {} });
    expect((await files(e, gone.asset))[0].link).toBe("broken");
    expect((await files(e, priv.asset))[0].link).toBe("broken");
    expect((await files(e, ok.asset))[0].link).toBe("ok");
  });
});
