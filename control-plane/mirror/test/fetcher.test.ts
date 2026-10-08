/**
 * The mirror fetcher's control-plane side (`src/mirrorfetcher.ts`): auth, leases, page HTML
 * parsed exactly as the Worker parses it, presigned uploads, and the Worker handing hosts that
 * refuse it to the fetcher. Against the whole recorded careless-beta page.
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { dispatch, readPageJobs, runMirror, upsertPost, writeMirrorVersion, type Category, type DownloadOption, type MirrorJob } from "../../src/mirror";
import { mirrorFile } from "../../src/mirrorfetch";
import { FETCHER_PREFIX, fetcherRoutes, parseJobId } from "../../src/mirrorfetcher";
import { fetcherRouter, hostIn } from "../../src/fetcherroute";
import { d1 } from "../../test/d1sqlite";
import { fakeBucket, fakeFetch, fakeQueue, nodeHasher, sha256 } from "../../test/modfakes";

const FIX = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "test", "fixtures", "mods");
const CARELESS = readFileSync(join(FIX, "careless-beta.full.html"), "utf8");
const TOKEN = "t".repeat(40);
const NOW = 1_000_000;

const TREE = new Map<number, Category>(
  [
    { id: 118, name: "Uploads", parent: 0 },
    { id: 22, name: "Tracks", parent: 118 },
    { id: 301, name: "Intermediate", parent: 22 },
  ].map((c) => [c.id, c]),
);

const POST = {
  id: 191518,
  slug: "careless-beta",
  link: "https://mxb-mods.com/careless-beta/",
  modified: "2026-10-07T19:53:23",
  title: { rendered: "Careless (beta)" },
  content: { rendered: "<p>Will be adding more assets soon. I hope you enjoy!</p>" },
  categories: [301],
};

type TestEnv = Env & { ASSET_MIRROR: ReturnType<typeof fakeBucket>; MIRROR_QUEUE: ReturnType<typeof fakeQueue<MirrorJob>> };

function env(extra: Record<string, unknown> = {}): TestEnv {
  return {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue<MirrorJob>(),
    MXB_MIRROR: "on",
    MXB_ASSETS_CDN: "https://cdn.mxbsecure.com",
    MIRROR_FETCHER_TOKEN: TOKEN,
    MIRROR_FETCHER_HOSTS: "mxb-mods.com mediafire.com",
    R2_ACCESS_KEY_ID: "test-key",
    R2_SECRET_ACCESS_KEY: "test-secret",
    R2_S3_ENDPOINT: "https://account.r2.example",
    MXB_ASSETS_BUCKET: "mxb-assets",
    ...extra,
  } as unknown as TestEnv;
}

async function call(e: Env, action: string, body: unknown, opts: { token?: string | null; now?: number } = {}) {
  const url = `https://api.mxbsecure.com${FETCHER_PREFIX}${action}`;
  const headers: Record<string, string> = { "content-type": "application/json" };
  const token = opts.token === undefined ? TOKEN : opts.token;
  if (token !== null) headers.authorization = `Bearer ${token}`;
  const res = await fetcherRoutes(new Request(url, { method: "POST", headers, body: JSON.stringify(body) }), new URL(url), e, opts.now ?? NOW);
  return { status: res.status, body: (await res.json()) as any };
}

function webp(tag: string): Uint8Array {
  const enc = new TextEncoder();
  return new Uint8Array([...enc.encode("RIFF"), 8, 0, 0, 0, ...enc.encode("WEBPVP8 "), ...enc.encode(tag)]);
}

async function seedPost(e: Env): Promise<number> {
  await quietDiscovery(e);
  await upsertPost(e, POST, TREE, 0);
  return (await e.DB.prepare("SELECT id FROM mod_assets WHERE source_ref = ?").bind(POST.id).first<{ id: number }>())!.id;
}

/** Discovery's next round far off, so a lease holds only the jobs a test is about. */
async function quietDiscovery(e: Env): Promise<void> {
  const st = { seq: 0, phase: "idle", catPage: 1, cats: [], listed: 0, roundAt: 0, leasedUntil: 0, nextAt: NOW * 1000 };
  await e.DB.prepare("INSERT OR REPLACE INTO mirror_state (key, value) VALUES ('fetcher_discovery', ?)").bind(JSON.stringify(st)).run();
}

/** A mirrored track whose page offers `links`, page read, files `status`. */
async function seedFiles(e: Env, links: string[], status: string): Promise<number> {
  const id = await seedPost(e);
  await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok', page_rev = 99 WHERE id = ?").bind(id).run();
  const opts: DownloadOption[] = links.map((url) => ({ url, host: "h", label: "l", isDefault: false, isServer: false }));
  const v = await writeMirrorVersion(e, id, null, opts, 0);
  await e.DB.prepare("UPDATE mod_files SET status = ? WHERE version_id = ?").bind(status, v).run();
  return v;
}

describe("fetcher auth", () => {
  it("is off without a token, and refuses a wrong or missing one", async () => {
    expect((await call(env({ MIRROR_FETCHER_TOKEN: undefined }), "lease", {})).status).toBe(503);
    expect((await call(env({ MIRROR_FETCHER_TOKEN: "short" }), "lease", {}, { token: "short" })).status).toBe(503);
    expect((await call(env(), "lease", {}, { token: null })).status).toBe(401);
    expect((await call(env(), "lease", {}, { token: "x".repeat(40) })).status).toBe(401);
    expect((await call(env(), "lease", {})).status).toBe(200);
  });

  it("takes POST and a JSON object only", async () => {
    const e = env();
    const url = `https://api.mxbsecure.com${FETCHER_PREFIX}lease`;
    const get = await fetcherRoutes(new Request(url, { headers: { authorization: `Bearer ${TOKEN}` } }), new URL(url), e, NOW);
    expect(get.status).toBe(405);
    expect((await call(e, "lease", [1, 2])).status).toBe(400);
    expect((await call(e, "result", { job: "nonsense" })).status).toBe(400);
    expect((await call(e, "nothing", { job: "page:1" })).status).toBe(404);
  });

  it("reads job ids strictly", () => {
    expect(parseJobId("page:12")).toEqual({ kind: "page", id: 12 });
    expect(parseJobId("file:3:0:2")).toEqual({ kind: "file", version: 3, idx: 0, part: 2 });
    expect(parseJobId("page:1; DROP")).toBeNull();
    expect(parseJobId(7)).toBeNull();
  });
});

describe("routing", () => {
  it("matches configured hosts and their subdomains only", async () => {
    expect(hostIn("download1234.mediafire.com", ["mediafire.com"])).toBe(true);
    expect(hostIn("notmediafire.com", ["mediafire.com"])).toBe(false);
    const route = await fetcherRouter(env());
    expect(route("https://www.mediafire.com/file/abc/x.pkz/file")).toBe(true);
    expect(route("https://drive.google.com/file/d/x/view")).toBe(false);
    expect((await fetcherRouter(env({ MIRROR_FETCHER_HOSTS: "" })))("https://www.mediafire.com/x")).toBe(false);
  });

  it("dispatch hands fetcher hosts to the fetcher and the rest to the queue", async () => {
    const e = env();
    const v = await seedFiles(e, ["https://www.mediafire.com/file/abcdefghijk/t.pkz/file", "https://cdn.example/t.zip"], "pending");
    expect(await dispatch(e, NOW)).toBe(2);
    const rows = await e.DB.prepare("SELECT idx, status FROM mod_files WHERE version_id = ? ORDER BY idx").bind(v).all();
    expect(rows.results).toEqual([
      { idx: 0, status: "fetcher" },
      { idx: 1, status: "queued" },
    ]);
    expect(e.MIRROR_QUEUE.sent).toEqual([{ kind: "file", version: v, idx: 1, part: 0 }]);
  });

  it("a host that answers the Worker 403 goes to the fetcher, and is remembered", async () => {
    const e = env();
    const v = await seedFiles(e, ["https://files.example.net/t.pkz"], "queued");
    const f = fakeFetch([[/files\.example\.net/, () => new Response("no", { status: 403 })]]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher, now: NOW });
    expect(await e.DB.prepare("SELECT status, attempts FROM mod_files").first()).toEqual({ status: "fetcher", attempts: 0 });
    expect((await fetcherRouter(e))("https://files.example.net/other.zip")).toBe(true);
  });

  it("without a fetcher, a 403 is the failure it always was", async () => {
    const e = env({ MIRROR_FETCHER_HOSTS: "" });
    const v = await seedFiles(e, ["https://files.example.net/t.pkz"], "queued");
    const f = fakeFetch([[/files\.example\.net/, () => new Response("no", { status: 403 })]]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher, now: NOW });
    expect(await e.DB.prepare("SELECT status FROM mod_files").first()).toEqual({ status: "failed" });
  });

  it("the Worker queues no page reads while pages go via the fetcher, and hands back stragglers", async () => {
    const e = env();
    const id = await seedPost(e);
    await e.DB.prepare("UPDATE mod_assets SET page_status = 'queued', page_due_at = ? WHERE id = ?").bind(NOW + 60_000, id).run();
    const f = fakeFetch([]);
    const r = await readPageJobs(e, [id], { now: NOW, fetch: f, wait: async () => {} });
    expect(r.deferred).toBe(1);
    expect(f.calls).toEqual([]);
    expect(await e.DB.prepare("SELECT page_status FROM mod_assets").first()).toEqual({ page_status: "due" });
  });
});

describe("leases", () => {
  it("hands out files first, then due pages, once each until the lease runs out", async () => {
    const e = env();
    const v = await seedFiles(e, ["https://www.mediafire.com/file/abcdefghijk/t.pkz/file"], "fetcher");
    await upsertPost(e, { ...POST, id: 5, slug: "five", link: "https://mxb-mods.com/five/" }, TREE, 0);
    const first = await call(e, "lease", { max: 5 });
    expect(first.body.jobs).toEqual([
      expect.objectContaining({ id: `file:${v}:0:0`, kind: "file", url: "https://www.mediafire.com/file/abcdefghijk/t.pkz/file", folder_allowed: true }),
      { id: expect.stringMatching(/^page:\d+$/), kind: "page", url: "https://mxb-mods.com/five/" },
    ]);
    expect((await call(e, "lease", { max: 5 })).body.jobs).toEqual([]);
    const later = await call(e, "lease", { max: 5 }, { now: NOW + 2 * 3600_000 });
    expect(later.body.jobs).toHaveLength(2);
  });

  it("leases no pages while pages are the Worker's", async () => {
    const e = env({ MIRROR_FETCHER_HOSTS: "mediafire.com" });
    await seedPost(e);
    expect((await call(e, "lease", {})).body.jobs).toEqual([]);
  });

  it("caps a lease", async () => {
    const e = env();
    for (let i = 1; i <= 15; i++) await upsertPost(e, { ...POST, id: i, slug: `p${i}`, link: `https://mxb-mods.com/p${i}/` }, TREE, 0);
    expect((await call(e, "lease", { max: 100 })).body.jobs).toHaveLength(10);
  });
});

describe("pages from the fetcher", () => {
  async function leased(e: TestEnv): Promise<{ id: number; job: string }> {
    const id = await seedPost(e);
    const { body } = await call(e, "lease", { max: 1 });
    expect(body.jobs).toEqual([{ id: `page:${id}`, kind: "page", url: POST.link }]);
    return { id, job: body.jobs[0].id };
  }

  it("parses the HTML exactly as the Worker's own read does", async () => {
    // The Worker reads the page itself…
    const w = env({ MIRROR_FETCHER_HOSTS: "" });
    const wid = await seedPost(w);
    const site = fakeFetch([
      [/\/robots\.txt$/, () => new Response("")],
      [/careless-beta\/$/, () => new Response(CARELESS, { headers: { "content-type": "text/html" } })],
      [/\.webp$/, (req) => new Response(webp(req.url), { headers: { "content-type": "image/webp" } })],
    ]);
    await w.DB.prepare("UPDATE mod_assets SET page_status = 'queued', page_due_at = ? WHERE id = ?").bind(NOW + 60_000, wid).run();
    await readPageJobs(w, [wid], { now: NOW, fetch: site, wait: async () => {} });

    // …and the fetcher hands the same HTML in, then uploads the pictures it is asked for.
    const e = env();
    const { id, job } = await leased(e);
    const res = await call(e, "result", { job, html: CARELESS });
    expect(res.status).toBe(200);
    expect(res.body.images.map((i: { purpose: string }) => i.purpose)).toEqual(["thumb", "image", "image", "image", "image", "image"]);
    const uploaded = [];
    for (const img of res.body.images) {
      const bytes = webp(img.src);
      const up = { job, purpose: img.purpose, src: img.src, sha256: await sha256(bytes), size: bytes.length, content_type: "image/webp" };
      const u = await call(e, "upload-url", up);
      expect(u.status).toBe(200);
      expect(u.body.key).toMatch(img.purpose === "thumb" ? /^thumbs\/[0-9a-f]{64}\.webp$/ : /^img\/[0-9a-f]{64}\.webp$/);
      e.ASSET_MIRROR.objects.set(u.body.key, { bytes });
      uploaded.push(up);
    }
    expect((await call(e, "done", { job, images: uploaded })).body).toEqual({ ok: true, rejected: [] });

    const cols = "type, author, page_status, page_rev, body, thumb_src, (thumb_key IS NOT NULL) AS has_thumb";
    const a = await w.DB.prepare(`SELECT ${cols} FROM mod_assets WHERE id = ?`).bind(wid).first();
    const b = await e.DB.prepare(`SELECT ${cols} FROM mod_assets WHERE id = ?`).bind(id).first();
    expect(b).toEqual(a);
    expect(b).toMatchObject({ page_status: "ok", author: "Zattari", has_thumb: 1 });
    const files = "SELECT idx, url, host, label, is_server, is_default, status FROM mod_files ORDER BY idx";
    expect((await e.DB.prepare(files).all()).results).toEqual((await w.DB.prepare(files).all()).results);
    expect((await e.DB.prepare("SELECT label FROM mod_versions").first())).toEqual({ label: "Beta 19" });
    const imgs = "SELECT idx, sha256, src, width, height FROM mod_asset_images ORDER BY idx";
    expect((await e.DB.prepare(imgs).all()).results).toEqual((await w.DB.prepare(imgs).all()).results);
    expect((await e.DB.prepare(imgs).all()).results).toHaveLength(5);
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mirror_state WHERE key LIKE 'fetcher-page:%'").first()).toEqual({ n: 0 });
  });

  it("presigns a PUT against mxb-assets, signed with the headers the box must send", async () => {
    const e = env();
    const { job } = await leased(e);
    const { body } = await call(e, "result", { job, html: CARELESS });
    const img = body.images[1];
    const bytes = webp("x");
    const u = await call(e, "upload-url", { job, purpose: "image", src: img.src, sha256: await sha256(bytes), size: bytes.length, content_type: "image/webp" });
    const url = new URL(u.body.url);
    expect(url.origin + url.pathname).toBe(`https://account.r2.example/mxb-assets/${u.body.key}`);
    expect(url.searchParams.get("X-Amz-Signature")).toMatch(/^[0-9a-f]{64}$/);
    expect(url.searchParams.get("X-Amz-SignedHeaders")).toContain("cache-control");
    expect(u.body).toMatchObject({ method: "PUT", headers: { "cache-control": "public, max-age=31536000, immutable", "content-type": "image/webp" } });
    // Only the pictures the page asked for, of a type we keep, under the cap.
    expect((await call(e, "upload-url", { job, purpose: "image", src: "https://evil.example/x.png", sha256: "a".repeat(64), size: 9, content_type: "image/png" })).status).toBe(409);
    expect((await call(e, "upload-url", { job, purpose: "image", src: img.src, sha256: "a".repeat(64), size: 9, content_type: "image/svg+xml" })).status).toBe(400);
    expect((await call(e, "upload-url", { job, purpose: "image", src: img.src, sha256: "a".repeat(64), size: 50 * 1024 ** 2, content_type: "image/webp" })).status).toBe(400);
    // Without the R2 credentials there is nothing to presign with.
    const bare = env({ R2_S3_ENDPOINT: undefined });
    bare.DB = e.DB;
    expect((await call(bare, "upload-url", { job, purpose: "image", src: img.src, sha256: await sha256(bytes), size: bytes.length, content_type: "image/webp" })).status).toBe(503);
  });

  it("drops a picture that isn't one, and still finishes the page", async () => {
    const e = env();
    const { id, job } = await leased(e);
    const { body } = await call(e, "result", { job, html: CARELESS });
    const img = body.images[1];
    const fake = new TextEncoder().encode("<svg onload=alert(1)>");
    const up = { purpose: "image", src: img.src, sha256: await sha256(fake), size: fake.length, content_type: "image/webp" };
    const u = await call(e, "upload-url", { job, ...up });
    e.ASSET_MIRROR.objects.set(u.body.key, { bytes: fake });
    const done = await call(e, "done", { job, images: [up] });
    expect(done.body.rejected).toEqual([{ src: img.src, error: "not the picture it claims to be" }]);
    expect(e.ASSET_MIRROR.objects.has(u.body.key)).toBe(false);
    expect(await e.DB.prepare("SELECT page_status FROM mod_assets WHERE id = ?").bind(id).first()).toEqual({ page_status: "ok" });
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_asset_images").first()).toEqual({ n: 0 });
  });

  it("finishes at once when every picture is already held", async () => {
    const e = env();
    const { id, job } = await leased(e);
    const html = CARELESS.replace(/<img\b[^>]*>/g, "").replace(/<meta property="og:image"[^>]*>/g, "");
    expect((await call(e, "result", { job, html })).body).toEqual({ ok: true, images: [] });
    expect(await e.DB.prepare("SELECT page_status, page_rev > 0 AS rev FROM mod_assets WHERE id = ?").bind(id).first()).toEqual({ page_status: "ok", rev: 1 });
  });

  it("files failures the way the Worker would", async () => {
    const e = env();
    const { id, job } = await leased(e);
    expect((await call(e, "result", { job, error: "site answered 403", status: 403 })).status).toBe(200);
    expect(await e.DB.prepare("SELECT page_status, page_attempts, page_due_at > ? AS later FROM mod_assets").bind(NOW).first()).toEqual({
      page_status: "due",
      page_attempts: 0,
      later: 1,
    });
    // Not leased any more: a late answer is refused.
    expect((await call(e, "result", { job, html: CARELESS })).status).toBe(409);

    await e.DB.prepare("UPDATE mod_assets SET page_due_at = 0").run();
    const again = (await call(e, "lease", {})).body.jobs[0].id;
    await call(e, "result", { job: again, error: "page answered 404", status: 404 });
    expect(await e.DB.prepare("SELECT page_status FROM mod_assets WHERE id = ?").bind(id).first()).toEqual({ page_status: "gone" });
  });

  it("a challenge page is retried, not parsed", async () => {
    const e = env();
    const { job } = await leased(e);
    await call(e, "result", { job, html: "<html><title>Just a moment...</title><script>cf_chl_opt</script></html>" });
    expect(await e.DB.prepare("SELECT page_status, page_error FROM mod_assets").first()).toEqual({ page_status: "retry", page_error: "challenge page" });
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_versions").first()).toEqual({ n: 0 });
  });
});

describe("files from the fetcher", () => {
  const MF = "https://www.mediafire.com/file/abcdefghijk/Careless.pkz/file";

  it("uploads by SHA-256 to the key the Worker would have used, and records it", async () => {
    const e = env();
    const v = await seedFiles(e, [MF], "fetcher");
    const job = (await call(e, "lease", {})).body.jobs[0].id;
    const bytes = new TextEncoder().encode("a track archive");
    const up = { job, sha256: await sha256(bytes), size: bytes.length, filename: "Careless.pkz", content_type: "application/octet-stream" };
    const u = await call(e, "upload-url", up);
    expect(u.body).toMatchObject({ have: false, key: `tracks/${up.sha256}`, method: "PUT" });
    expect(u.body.headers["content-disposition"]).toBe("attachment; filename*=UTF-8''Careless.pkz");
    expect(new URL(u.body.url).searchParams.get("X-Amz-SignedHeaders")).toContain("content-disposition");

    // Done before the bytes are there: refused.
    expect((await call(e, "done", up)).body).toEqual({ error: "not in R2" });
    e.ASSET_MIRROR.objects.set(u.body.key, { bytes });
    expect((await call(e, "done", up)).body).toEqual({ ok: true });
    expect(await e.DB.prepare("SELECT status, sha256, filename FROM mod_files WHERE version_id = ?").bind(v).first()).toEqual({
      status: "done",
      sha256: up.sha256,
      filename: "Careless.pkz",
    });
    expect(await e.DB.prepare("SELECT bucket, r2_key, size FROM mod_blobs").first()).toEqual({ bucket: "public", r2_key: u.body.key, size: bytes.length });
  });

  it("never uploads a file we already hold, and refuses a size that doesn't match", async () => {
    const e = env();
    await seedFiles(e, [MF, "https://www.mediafire.com/file/bbbbbbbbbbb/b.pkz/file"], "fetcher");
    await e.DB.prepare("INSERT INTO mod_blobs (sha256, bucket, r2_key, size, first_seen) VALUES (?, 'public', ?, 3, 0)").bind("b".repeat(64), `tracks/${"b".repeat(64)}`).run();
    const [one, two] = (await call(e, "lease", { max: 2 })).body.jobs.map((j: { id: string }) => j.id);
    const have = await call(e, "upload-url", { job: one, sha256: "b".repeat(64), size: 3, filename: "x.pkz" });
    expect(have.body).toEqual({ have: true, key: `tracks/${"b".repeat(64)}` });
    expect((await call(e, "done", { job: one, sha256: "b".repeat(64), size: 3, filename: "x.pkz" })).body).toEqual({ ok: true });

    const u = await call(e, "upload-url", { job: two, sha256: "c".repeat(64), size: 10, filename: "b.pkz" });
    e.ASSET_MIRROR.objects.set(u.body.key, { bytes: new Uint8Array(4) });
    expect((await call(e, "done", { job: two, sha256: "c".repeat(64), size: 10, filename: "b.pkz" })).body).toEqual({ error: "R2 holds 4 bytes, not 10" });
    expect(e.ASSET_MIRROR.objects.has(u.body.key)).toBe(false);
  });

  it("locked content never goes this way", async () => {
    const e = env();
    await seedFiles(e, [MF], "fetcher");
    const job = (await call(e, "lease", {})).body.jobs[0].id;
    expect((await call(e, "upload-url", { job, sha256: "d".repeat(64), size: 5, filename: "x.mxbsecure" })).status).toBe(422);
    expect(await e.DB.prepare("SELECT status FROM mod_files").first()).toEqual({ status: "runner" });
  });

  it("a folder the box listed becomes parts, each the fetcher's", async () => {
    const e = env();
    const v = await seedFiles(e, ["https://www.mediafire.com/folder/9dhrz4bkzcnzo/I40"], "fetcher");
    const job = (await call(e, "lease", {})).body.jobs[0].id;
    const folder = {
      name: "I40",
      files: [
        { rel: "I40 MX.pkz", url: "https://www.mediafire.com/file/aaaaaaaaaaa/I40_MX.pkz/file", size: 10 },
        { rel: "server/I40 MX.pkz", url: "https://www.mediafire.com/file/bbbbbbbbbbb/I40_MX.pkz/file" },
        { rel: "bad", url: "javascript:alert(1)" },
      ],
    };
    expect((await call(e, "result", { job, folder })).body).toEqual({ ok: true });
    const rows = await e.DB.prepare("SELECT part, rel, status FROM mod_files WHERE version_id = ? ORDER BY part").bind(v).all();
    expect(rows.results).toEqual([
      { part: 0, rel: null, status: "folder" },
      { part: 1, rel: "I40/I40 MX.pkz", status: "fetcher" },
      { part: 2, rel: "I40/server/I40 MX.pkz", status: "fetcher" },
    ]);
    const parts = (await call(e, "lease", { max: 5 })).body.jobs;
    expect(parts.map((j: { id: string; filename: string; folder_allowed: boolean }) => [j.id, j.filename, j.folder_allowed])).toEqual([
      [`file:${v}:0:1`, "I40 MX.pkz", false],
      [`file:${v}:0:2`, "I40 MX.pkz", false],
    ]);
  });

  it("a failure backs off and stays the fetcher's; a permanent one, or too many, fails", async () => {
    const e = env();
    await seedFiles(e, [MF], "fetcher");
    let job = (await call(e, "lease", {})).body.jobs[0].id;
    await call(e, "result", { job, error: "MediaFire: answered 429", retry_after_ms: 3 * 3600_000 });
    expect(await e.DB.prepare("SELECT status, attempts, due_at >= ? AS later FROM mod_files").bind(NOW + 3 * 3600_000).first()).toEqual({
      status: "fetcher",
      attempts: 1,
      later: 1,
    });
    // The box backing the host off is no attempt.
    await e.DB.prepare("UPDATE mod_files SET due_at = 0").run();
    job = (await call(e, "lease", {})).body.jobs[0].id;
    await call(e, "result", { job, error: "site answered 403", status: 403, deferred: true, retry_after_ms: 1000 });
    expect(await e.DB.prepare("SELECT status, attempts, due_at FROM mod_files").first()).toEqual({
      status: "fetcher",
      attempts: 1,
      due_at: NOW + 30 * 60_000,
    });
    await e.DB.prepare("UPDATE mod_files SET due_at = 0").run();
    job = (await call(e, "lease", {})).body.jobs[0].id;
    await call(e, "result", { job, error: "MediaFire: the file no longer exists", permanent: true });
    expect(await e.DB.prepare("SELECT status FROM mod_files").first()).toEqual({ status: "failed" });
  });
});

describe("discovery from the fetcher", () => {
  const listJob = async (e: Env, now = NOW) =>
    ((await call(e, "lease", { max: 10 }, { now })).body.jobs as { id: string; kind: string; url: string }[]).find((j) => j.kind === "list");
  const answer = (e: Env, job: string, body: unknown, now = NOW, status = 200) =>
    call(e, "result", { job, status, body: JSON.stringify(body) }, { now });

  it("walks a round, one request at a time, and parses what comes back as the Worker would", async () => {
    // The Worker's own walk…
    const w = env({ MIRROR_FETCHER_HOSTS: "" });
    const site = fakeFetch([
      [/\/robots\.txt$/, () => new Response("")],
      [/\/wp-json\/wp\/v2\/categories/, () => Response.json([...TREE.values()])],
      [/orderby=modified/, () => Response.json([POST])],
      [/orderby=id/, () => Response.json([{ id: POST.id }])],
    ]);
    await runMirror(w, { now: NOW, fetch: site, wait: async () => {} });

    // …and the same answers handed in by the fetcher.
    const e = env();
    const cats = await listJob(e);
    expect(cats?.url).toContain("/wp-json/wp/v2/categories");
    // One in flight at a time.
    expect(await listJob(e)).toBeUndefined();
    expect((await answer(e, cats!.id, [...TREE.values()])).status).toBe(200);
    const listing = await listJob(e);
    expect(listing?.url).toContain("orderby=modified");
    await answer(e, listing!.id, [POST]);
    const sweep = await listJob(e);
    expect(sweep?.url).toContain("orderby=id");
    await answer(e, sweep!.id, [{ id: POST.id }]);
    // The round is over until ten minutes after it began.
    expect(await listJob(e, NOW + 60_000)).toBeUndefined();

    // page_status aside: the Worker queued the page read, the fetcher leased it.
    const cols = "source_ref, slug, title, type, bike, categories, description, source_url, modified, last_seen";
    expect((await e.DB.prepare(`SELECT ${cols} FROM mod_assets`).all()).results).toEqual(
      (await w.DB.prepare(`SELECT ${cols} FROM mod_assets`).all()).results,
    );
    const state = "SELECT value FROM mirror_state WHERE key = 'listing'";
    expect(await e.DB.prepare(state).first()).toEqual(await w.DB.prepare(state).first());

    // The next round: the tree is cached a day, so it starts at the listing, from the high-water mark.
    const next = await listJob(e, NOW + 10 * 60_000);
    expect(next?.url).toContain("modified_after=");
  });

  it("the Worker sends the site nothing while it is routed to the fetcher", async () => {
    const e = env();
    const f = fakeFetch([]);
    await runMirror(e, { now: NOW, fetch: f, wait: async () => {} });
    expect(f.calls).toEqual([]);
  });

  it("a refusal waits and asks the same step again; a late answer is refused", async () => {
    const e = env();
    const first = await listJob(e);
    await call(e, "result", { job: first!.id, error: "site answered 403", status: 403, deferred: true, retry_after_ms: 1000 });
    expect(await listJob(e, NOW + 60_000)).toBeUndefined();
    const again = await listJob(e, NOW + 31 * 60_000);
    expect(again?.url).toBe(first!.url);
    expect((await answer(e, first!.id, [], NOW + 31 * 60_000)).status).toBe(409);
    // A lease that ran out is handed out again.
    const later = await listJob(e, NOW + 45 * 60_000);
    expect(later?.url).toBe(first!.url);
    expect(later?.id).not.toBe(again!.id);
  });

  it("a page past the end (400) ends the walk; a challenge page waits", async () => {
    const e = env();
    const cats = await listJob(e);
    await answer(e, cats!.id, [...TREE.values()]);
    const listing = await listJob(e);
    await answer(e, listing!.id, { code: "rest_post_invalid_page_number" }, NOW, 400);
    const sweep = await listJob(e);
    expect(sweep?.url).toContain("orderby=id");
    await call(e, "result", { job: sweep!.id, status: 200, body: "<html>Just a moment...</html>" });
    expect(await listJob(e, NOW + 60_000)).toBeUndefined();
    expect((await listJob(e, NOW + 31 * 60_000))?.url).toContain("orderby=id");
  });
});
