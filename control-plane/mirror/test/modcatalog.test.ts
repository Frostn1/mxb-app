/**
 * The catalogue's public ids, pictures, bylines and download links, against recorded
 * mxb-mods.com samples (`test/fixtures/mods/modded-surron.*`, trimmed, recorded 2026-10-07).
 */
import { describe, expect, it } from "vitest";
import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  parseAuthor,
  parseDownloads,
  parseImage,
  listingUrl,
  publishedOf,
  restAuthor,
  runMirror,
  sniffImage,
  thumbOf,
  upsertPost,
  type Category,
  type Post,
} from "../../src/mirror";
import { isPublicId, modPath, modSlug, newPublicId } from "../../src/modids";
import { publicModRoutes } from "../../src/modapi";
import { addAccount, d1 } from "../../test/d1sqlite";
import { drainPages, fakeBucket, fakeFetch, fakeQueue } from "../../test/modfakes";
import { setThumb, type Uploader } from "../../src/uploads";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIX = join(HERE, "..", "..", "test", "fixtures", "mods");
const MIGRATIONS = join(HERE, "..", "..", "migrations");
const html = readFileSync(join(FIX, "modded-surron.html"), "utf8");
const rest = JSON.parse(readFileSync(join(FIX, "modded-surron.rest.json"), "utf8")) as Post[];

const TREE = new Map<number, Category>(
  [
    { id: 118, name: "Uploads", parent: 0 },
    { id: 29, name: "Bikes", parent: 118 },
    { id: 45, name: "New Bikes", parent: 29 },
    { id: 516, name: "Beta 19", parent: 0 },
  ].map((c) => [c.id, c]),
);

/** A WebP's first bytes and some body: what the thumbnail copy checks and stores. */
const WEBP = new Uint8Array([...new TextEncoder().encode("RIFF"), 8, 0, 0, 0, ...new TextEncoder().encode("WEBPVP8 "), 1, 2, 3]);

function env() {
  return {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue(),
    MXB_MIRROR: "on",
    MXB_ASSETS_CDN: "https://cdn.mxbsecure.com",
  } as unknown as Env & { ASSET_MIRROR: ReturnType<typeof fakeBucket> };
}

const get = (e: Env, path: string, method = "GET") =>
  publicModRoutes(new Request(`https://api.mxbsecure.com${path}`, { method }), new URL(`https://api.mxbsecure.com${path}`), e);

describe("public ids", () => {
  it("are v4 UUIDs, different every time", () => {
    const ids = new Set(Array.from({ length: 2000 }, newPublicId));
    expect(ids.size).toBe(2000);
    for (const id of ids) expect(isPublicId(id)).toBe(true);
    expect(isPublicId("21")).toBe(false);
    expect(isPublicId("00000000-0000-1000-8000-000000000000")).toBe(false);
  });

  it("every new asset gets one, unique and indexed", async () => {
    const e = env();
    for (let i = 1; i <= 120; i++) {
      await upsertPost(e, { id: i, slug: `p${i}`, link: `https://mxb-mods.com/p${i}/`, modified: "2026-01-01T00:00:00", title: { rendered: `P ${i}` } }, TREE, 1);
    }
    // An insert that names none still gets one, from the trigger.
    await e.DB.prepare(
      "INSERT INTO mod_assets (source, title, type, modified, first_seen, last_seen) VALUES ('upload', 'x', 'other', '', 0, 0)",
    ).run();
    const { results } = await e.DB.prepare("SELECT public_id FROM mod_assets").all<{ public_id: string }>();
    expect(results).toHaveLength(121);
    expect(new Set(results.map((r) => r.public_id)).size).toBe(121);
    expect(results.every((r) => isPublicId(r.public_id))).toBe(true);
    await expect(
      e.DB.prepare("UPDATE mod_assets SET public_id = (SELECT public_id FROM mod_assets WHERE id = 1) WHERE id = 2").run(),
    ).rejects.toThrow(/UNIQUE/);
  });

  it("makes readable addresses", () => {
    const id = "3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c";
    expect(modSlug("2026 RedBull KTM — FACTORY Racing!")).toBe("2026-redbull-ktm-factory-racing");
    expect(modSlug("***")).toBe("mod");
    expect(modPath("HillsFord MX park", id)).toBe(`/mods/hillsford-mx-park-${id}`);
  });
});

describe("the migration's backfill", () => {
  it("gives every existing row a unique id and queues pages missing a picture, byline or files", () => {
    const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
    const db = new DatabaseSync(":memory:");
    const files = readdirSync(MIGRATIONS).filter((f) => f.endsWith(".sql")).sort();
    const at = files.indexOf("0058_mod_public_ids.sql");
    expect(at).toBeGreaterThan(0);
    for (const f of files.slice(0, at)) db.exec(readFileSync(join(MIGRATIONS, f), "utf8"));
    const ins = db.prepare(
      `INSERT INTO mod_assets (source, source_ref, title, type, modified, first_seen, last_seen, page_status, author)
       VALUES ('mirror', ?, 't', 'other', '', 0, 0, ?, ?)`,
    );
    for (let i = 1; i <= 50; i++) ins.run(i, i <= 40 ? "due" : "ok", i === 50 ? "someone" : null);
    for (const f of files.slice(at)) db.exec(readFileSync(join(MIGRATIONS, f), "utf8"));
    const rows = db.prepare("SELECT public_id, page_status FROM mod_assets").all() as { public_id: string; page_status: string }[];
    expect(new Set(rows.map((r) => r.public_id)).size).toBe(50);
    expect(rows.every((r) => isPublicId(r.public_id))).toBe(true);
    expect(rows.filter((r) => r.page_status === "due")).toHaveLength(50);
  });
});

describe("reading a recorded mxb-mods post", () => {
  it("takes the picture from the REST listing, else the page", () => {
    expect(thumbOf(rest[0])).toBe("https://mxb-mods.com/wp-content/uploads/2025/05/mxbikes-2025-05-10-18-41-42-768x432.webp");
    expect(parseImage(html)).toBe("https://mxb-mods.com/wp-content/uploads/2025/05/mxbikes-2025-05-10-18-41-42.webp");
    expect(parseImage(`<div class="entry-content"><p>x</p><img class="a" src="https://mxb-mods.com/a.jpg"></div>`)).toBe(
      "https://mxb-mods.com/a.jpg",
    );
    expect(parseImage("<p>nothing</p>")).toBeNull();
  });

  it("takes the byline from the page; the REST user is refused and gives none", () => {
    expect(parseAuthor(html)).toBe("yourboyquinn361");
    expect(restAuthor(rest[0])).toBeNull();
    // The JSON-LD alone still names the author.
    const ldOnly = html.replace(/<div class="post-header">[\s\S]*?<\/div>/, "");
    expect(parseAuthor(ldOnly)).toBe("yourboyquinn361");
  });

  it("finds both download links", () => {
    expect(parseDownloads(html).map((d) => [d.host, d.isDefault])).toEqual([
      ["mediafire.com", false],
      ["drive.google.com", true],
    ]);
  });

  it("knows a picture by its bytes", () => {
    expect(sniffImage(WEBP)).toBe("image/webp");
    expect(sniffImage(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe("image/jpeg");
    expect(sniffImage(new TextEncoder().encode("<html>"))).toBeNull();
  });
});

describe("a whole run over the recorded post", () => {
  const ids = Array.from({ length: 100 }, (_, i) => 138269 - i);
  const site = () =>
    fakeFetch([
      [/\/robots\.txt$/, () => new Response("User-agent: *\nAllow: /ads.txt\n")],
      [/\/wp-json\/wp\/v2\/categories/, () => Response.json([...TREE.values()])],
      [/orderby=modified/, () => Response.json(rest)],
      // A full page of ids: 101 bound parameters in one statement used to stop the run here.
      [/orderby=id/, () => Response.json(ids.map((id) => ({ id })))],
      [/\/modded-surron-with-a-crf-500-body\/$/, () => new Response(html, { headers: { "content-type": "text/html" } })],
      [/\.webp$/, () => new Response(WEBP, { headers: { "content-type": "image/webp" } })],
    ]);

  it("records the files, the byline and a stored picture, and serves them by public id", async () => {
    const e = env();
    const f = site();
    await runMirror(e, { now: 10_000, fetch: f, wait: async () => {} });
    await drainPages(e, { now: 10_000, fetch: f, wait: async () => {} });
    const a = await e.DB.prepare("SELECT public_id, author, thumb_key, page_status FROM mod_assets").first<{
      public_id: string;
      author: string;
      thumb_key: string;
      page_status: string;
    }>();
    expect(a).toMatchObject({ author: "yourboyquinn361", page_status: "ok" });
    expect(a!.thumb_key).toMatch(/^thumbs\/[0-9a-f]{64}\.webp$/);
    expect(e.ASSET_MIRROR.objects.has(a!.thumb_key)).toBe(true);
    const { results } = await e.DB.prepare("SELECT host, url, status FROM mod_files ORDER BY idx").all();
    expect(results).toEqual([
      { host: "mediafire.com", url: "https://www.mediafire.com/file/uupx26riig16h94/modded+surron-20261005T030252Z-1-001.zip/file", status: "idle" },
      { host: "drive.google.com", url: "https://drive.google.com/drive/folders/1VBhJnjFkM4gPd2aYT_2Wf1L_kOnH_Z9N?dmr=1&ec=wgc-drive-hero-goto", status: "idle" },
    ]);

    const res = await get(e, `/v1/assets/${a!.public_id}`);
    const body = (await res!.json()) as { id: string; slug: string; author: string; thumb: string; files: { download: string }[] };
    expect(body).toMatchObject({ id: a!.public_id, slug: "modded-surron-with-a-crf-500-body", author: "yourboyquinn361" });
    expect(body.thumb).toBe(`https://cdn.mxbsecure.com/${a!.thumb_key}`);
    expect(body.files.map((f) => f.download)).toEqual([0, 1].map((i) => expect.stringContaining(`/v1/assets/${a!.public_id}/download/${i}`)));
    const search = (await (await get(e, "/v1/assets/search?q=surron"))!.json()) as { results: { id: string; author: string; thumb: string }[] };
    expect(search.results).toEqual([expect.objectContaining({ id: a!.public_id, author: "yourboyquinn361", thumb: body.thumb })]);
    expect(await (await get(e, "/v1/assets/stats"))!.json()).toEqual({
      total: 1,
      by_type: { paints: 0, bikes: 1, liveries: 0, kits: 0, tracks: 0, other: 0 },
    });
  });

  it("walks the listing whatever order the site answers in, then asks only for what changed", async () => {
    const e = env();
    const page = (n: number) =>
      Array.from({ length: 50 }, (_, i) => ({
        id: n * 100 + i,
        slug: `s${n}-${i}`,
        link: `https://mxb-mods.com/s${n}-${i}/`,
        modified: `2026-0${n}-01T00:00:${String(59 - i).padStart(2, "0")}`,
        title: { rendered: `S ${n} ${i}` },
      }));
    // Newest first, as mxb-mods.com answers no matter what it is asked.
    const all = [...page(3), ...page(2), ...page(1)];
    const f = fakeFetch([
      [/\/robots\.txt$/, () => new Response("")],
      [/\/wp-json\/wp\/v2\/categories/, () => Response.json([...TREE.values()])],
      [
        /orderby=modified/,
        (req) => {
          const u = new URL(req.url);
          const after = u.searchParams.get("modified_after") ?? "";
          const off = Number(u.searchParams.get("offset") ?? "0");
          return Response.json(all.filter((p) => p.modified > after).slice(off, off + Number(u.searchParams.get("per_page"))));
        },
      ],
      [/orderby=id/, () => Response.json([])],
    ]);
    // Two runs of four listing pages each cover 150 posts with the overlap.
    await runMirror(e, { now: 1, fetch: f, wait: async () => {} });
    expect((await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_assets").first())).toEqual({ n: 150 });
    const g = fakeFetch([[/orderby=modified/, () => Response.json([])], [/./, () => Response.json([])]]);
    await runMirror(e, { now: 2, fetch: g, wait: async () => {} });
    expect(g.calls.find((c) => c.includes("orderby=modified"))).toContain("modified_after=2026-03-01T00%3A00%3A58");
  });
});

describe("unlisted mods and old addresses", () => {
  const RIDER: Uploader = { id: "acc-rider", rider_name: "Frosty", steam_id: "steam-rider" };

  async function seeded() {
    const e = env();
    await addAccount(e.DB, RIDER.id, RIDER.rider_name);
    const pub = newPublicId();
    await e.DB.prepare(
      `INSERT INTO mod_assets (public_id, source, owner_account, visibility, title, type, modified, first_seen, last_seen)
       VALUES (?, 'upload', ?, 'unlisted', 'Secret livery', 'liveries', '2026-01-01', 0, 0)`,
    )
      .bind(pub, RIDER.id)
      .run();
    const v = await e.DB.prepare(
      "INSERT INTO mod_versions (asset_id, seq, state, created_at, created_by) VALUES (1, 1, 'live', 0, 'x') RETURNING id",
    ).first<{ id: number }>();
    await e.DB.prepare("INSERT INTO mod_blobs (sha256, bucket, r2_key, size, first_seen) VALUES ('aa', 'public', 'liveries/aa', 1, 0)").run();
    await e.DB.prepare("INSERT INTO mod_files (version_id, idx, status, sha256) VALUES (?, 0, 'done', 'aa')").bind(v!.id).run();
    await e.DB.prepare("UPDATE mod_assets SET current_version = ?").bind(v!.id).run();
    return { e, pub };
  }

  it("answers an unlisted mod only to its public id", async () => {
    const { e, pub } = await seeded();
    const s = (await (await get(e, "/v1/assets/search?q=secret"))!.json()) as { total: number };
    expect(s.total).toBe(0);
    expect((await get(e, `/v1/assets/${pub}`))!.status).toBe(200);
    expect((await get(e, `/v1/assets/${pub.toUpperCase()}`))!.status).toBe(200);
    expect((await get(e, `/v1/assets/${pub}/download/0`))!.status).toBe(302);
    // Counting through the internal ids finds nothing.
    for (const path of ["/v1/assets/1", "/v1/assets/1/download/0"]) expect((await get(e, path))!.status).toBe(404);
    expect((await get(e, "/v1/assets/1/report", "POST"))!.status).toBe(404);
    expect((await get(e, `/v1/assets/${newPublicId()}`))!.status).toBe(404);
  });

  it("lets the owner set a picture, checked by its bytes and size", async () => {
    const { e } = await seeded();
    const put = (body: BodyInit) => new Request("https://x", { method: "PUT", body });
    expect((await setThumb(put("not a picture"), 1, RIDER, e)).status).toBe(415);
    expect((await setThumb(put(new Uint8Array(3 * 1024 * 1024)), 1, RIDER, e)).status).toBe(413);
    expect((await setThumb(put(WEBP), 1, { ...RIDER, id: "acc-other" }, e)).status).toBe(404);
    const ok = await setThumb(put(WEBP), 1, RIDER, e);
    expect(ok.body).toEqual({ thumb: expect.stringMatching(/^https:\/\/cdn\.mxbsecure\.com\/thumbs\/[0-9a-f]{64}\.webp$/) });
  });
});

describe("publish dates", () => {
  const post = (id: number, extra: Partial<Post>): Post => ({
    id,
    slug: `p${id}`,
    link: `https://mxb-mods.com/p${id}/`,
    modified: "2026-10-07T00:00:00",
    title: { rendered: `P ${id}` },
    ...extra,
  });

  it("reads the post date from the listing, UTC first", () => {
    expect(publishedOf(post(1, { date: "2026-10-03T08:30:00", date_gmt: "2026-10-03T12:30:00" }))).toBe("2026-10-03T12:30:00Z");
    expect(publishedOf(post(1, { date: "2026-10-03T08:30:00" }))).toBe("2026-10-03T08:30:00Z");
    expect(publishedOf(post(1, { date_gmt: "0000-00-00T00:00:00" }))).toBeNull();
    expect(publishedOf(post(1, {}))).toBeNull();
    expect(listingUrl({ hwm: "", walk: null }).searchParams.get("_fields")).toContain("date_gmt");
  });

  it("stores it on the mirrored row, keeps it when a later listing lacks it, and keeps modified apart", async () => {
    const e = env();
    await upsertPost(e, post(1, { date_gmt: "2026-10-03T12:30:00" }), TREE, 5000);
    await upsertPost(e, post(1, { modified: "2026-10-08T00:00:00" }), TREE, 6000);
    const row = await e.DB.prepare("SELECT published, modified, first_seen FROM mod_assets").first();
    expect(row).toEqual({ published: "2026-10-03T12:30:00Z", modified: "2026-10-08T00:00:00", first_seen: 5000 });
  });

  it("returns published in search and detail, newest published first, with a fallback for unfilled rows", async () => {
    const e = env();
    await upsertPost(e, post(1, { date_gmt: "2026-01-01T00:00:00", modified: "2026-10-09T00:00:00" }), TREE, Date.parse("2026-10-01T00:00:00Z"));
    await upsertPost(e, post(2, { date_gmt: "2026-09-01T00:00:00", modified: "2026-02-01T00:00:00" }), TREE, Date.parse("2026-10-01T00:00:00Z"));
    // Not re-listed yet: sorts and shows by first_seen.
    await upsertPost(e, post(3, {}), TREE, Date.parse("2026-09-15T00:00:00Z"));
    const s = (await (await get(e, "/v1/assets/search"))!.json()) as { results: { title: string; published: string; updated: string }[] };
    expect(s.results.map((r) => r.title)).toEqual(["P 3", "P 2", "P 1"]);
    expect(s.results.map((r) => r.published)).toEqual(["2026-09-15T00:00:00Z", "2026-09-01T00:00:00Z", "2026-01-01T00:00:00Z"]);
    expect(s.results[2].updated).toBe("2026-10-09T00:00:00");
    const id = (await e.DB.prepare("SELECT public_id FROM mod_assets WHERE source_ref = 2").first<{ public_id: string }>())!.public_id;
    const d = (await (await get(e, `/v1/assets/${id}`))!.json()) as { published: string; updated: string };
    expect(d.published).toBe("2026-09-01T00:00:00Z");
    expect(d.updated).toBe("2026-02-01T00:00:00");
  });

  it("stamps an upload with its upload time", async () => {
    const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
    const db = new DatabaseSync(":memory:");
    const files = readdirSync(MIGRATIONS).filter((f) => f.endsWith(".sql")).sort();
    const at = files.indexOf("0066_mod_published.sql");
    expect(at).toBeGreaterThan(0);
    for (const f of files.slice(0, at)) db.exec(readFileSync(join(MIGRATIONS, f), "utf8"));
    const ins = db.prepare(
      `INSERT INTO mod_assets (source, source_ref, title, type, modified, first_seen, last_seen) VALUES (?, ?, 't', 'other', '', ?, 0)`,
    );
    ins.run("upload", null, Date.parse("2026-10-03T12:30:00Z"));
    ins.run("mirror", 7, 1000);
    // A finished walk's cursor, and one in flight.
    db.prepare("INSERT INTO mirror_state (key, value) VALUES ('listing', '{\"hwm\":\"2026-10-07T00:00:00\",\"walk\":null}'), ('sweep', '{}')").run();
    db.exec(readFileSync(join(MIGRATIONS, files[at]), "utf8"));
    const rows = db.prepare("SELECT source, published FROM mod_assets ORDER BY id").all();
    expect(rows).toEqual([
      { source: "upload", published: "2026-10-03T12:30:00Z" },
      { source: "mirror", published: null },
    ]);
    // The mirrored rows are filled by a full re-list: the cursor is gone, other state is not.
    expect(db.prepare("SELECT key FROM mirror_state").all()).toEqual([{ key: "sweep" }]);
  });
});
