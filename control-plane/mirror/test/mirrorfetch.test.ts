import { describe, expect, it } from "vitest";
import { upsertPost, writeMirrorVersion, type Category, type DownloadOption } from "../../src/mirror";
import { mirrorFile, multipartFrom, placement, PART_BYTES } from "../../src/mirrorfetch";
import { megaFileKey } from "../../src/mirrorhosts";
import { d1 } from "../../test/d1sqlite";
import { fakeBucket, fakeFetch, fakeQueue, fixture, nodeHasher, sha256 } from "../../test/modfakes";

const TREE = new Map<number, Category>([
  [29, { id: 29, name: "Bikes", parent: 0 }],
  [37, { id: 37, name: "Liveries", parent: 29 }],
]);

function env() {
  return {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue(),
    MXB_MIRROR: "on",
  } as unknown as Env & { ASSET_MIRROR: ReturnType<typeof fakeBucket>; ASSET_LOCKED: ReturnType<typeof fakeBucket> };
}

/** One mirrored livery whose page offers `links`, every file queued. */
async function seed(e: Env, links: string[], postId = 7): Promise<number> {
  await upsertPost(
    e,
    { id: postId, slug: `p${postId}`, link: `https://mxb-mods.com/p${postId}/`, modified: "2026-01-01T00:00:00", categories: [37], title: { rendered: "Red Bull KTM" } },
    TREE,
    0,
  );
  const a = await e.DB.prepare("SELECT id FROM mod_assets WHERE source_ref = ?").bind(postId).first<{ id: number }>();
  await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok' WHERE id = ?").bind(a!.id).run();
  const opts: DownloadOption[] = links.map((url) => ({ url, host: "h", label: "l", isDefault: false, isServer: false }));
  const v = await writeMirrorVersion(e, a!.id, null, opts, 0);
  await e.DB.prepare("UPDATE mod_files SET status = 'queued' WHERE version_id = ?").bind(v).run();
  return v;
}

const file = (bytes: Uint8Array, name = "Red Bull.pnt") =>
  new Response(bytes, {
    headers: {
      "content-type": "application/octet-stream",
      "content-length": String(bytes.length),
      "content-disposition": `attachment; filename="${name}"`,
    },
  });

describe("multipart streaming", () => {
  it("cuts a body into fixed parts whatever the chunking, and counts it", async () => {
    const b = fakeBucket();
    const total = PART_BYTES * 2 + 123;
    let sent = 0;
    const body = new ReadableStream<Uint8Array>({
      pull(c) {
        if (sent >= total) return c.close();
        const n = Math.min(3_000_001, total - sent);
        c.enqueue(new Uint8Array(n).fill(sent % 251));
        sent += n;
      },
    });
    const size = await multipartFrom(b as unknown as R2Bucket, "k", body);
    expect(size).toBe(total);
    expect(b.partSizes).toEqual([PART_BYTES, PART_BYTES, 123]);
    expect(b.objects.get("k")!.bytes.length).toBe(total);
  });
  it("aborts past the size cap", async () => {
    const b = fakeBucket();
    const body = new Blob([new Uint8Array(1000)]).stream();
    expect(await multipartFrom(b as unknown as R2Bucket, "k", body, { max: 999 })).toBe("too-big");
    expect(b.objects.size).toBe(0);
    expect(b.uploads.size).toBe(0);
  });
});

describe("placement", () => {
  it("puts server builds under server/, locked content in the private bucket", () => {
    expect(placement("tracks", true, "x.pkz")).toEqual({ bucket: "public", prefix: "server" });
    expect(placement("liveries", false, "x.pnt")).toEqual({ bucket: "public", prefix: "liveries" });
    expect(placement("kits", false, "x.mxbsecure")).toEqual({ bucket: "private", prefix: "locked" });
    expect(placement("weird", false, "x")).toEqual({ bucket: "public", prefix: "other" });
  });
});

describe("mirroring one file", () => {
  it("stores a direct file under its type by SHA-256, and records the blob", async () => {
    const e = env();
    const v = await seed(e, ["https://mxb-mods.com/wp-content/uploads/2023/08/Red-Bull.pnt"]);
    const bytes = new TextEncoder().encode("PNT\0 a paint");
    const f = fakeFetch([[/Red-Bull\.pnt/, () => file(bytes)]]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher, now: 5 });
    const sha = await sha256(bytes);
    expect(await e.DB.prepare("SELECT status, sha256, filename FROM mod_files").first()).toEqual({
      status: "done",
      sha256: sha,
      filename: "Red Bull.pnt",
    });
    expect(await e.DB.prepare("SELECT bucket, r2_key, size FROM mod_blobs").first()).toEqual({
      bucket: "public",
      r2_key: `liveries/${sha}`,
      size: bytes.length,
    });
    expect([...e.ASSET_MIRROR.objects.keys()]).toEqual([`liveries/${sha}`]);
    expect(e.ASSET_MIRROR.objects.get(`liveries/${sha}`)!.httpMetadata?.contentDisposition).toContain("Red%20Bull.pnt");
  });

  it("stores the same bytes once, however many posts link them", async () => {
    const e = env();
    const bytes = new TextEncoder().encode("same archive");
    const f = fakeFetch([[/./, () => file(bytes, "a.zip")]]);
    const v1 = await seed(e, ["https://x.example/a.zip"], 1);
    const v2 = await seed(e, ["https://y.example/b.zip"], 2);
    await mirrorFile(e, { kind: "file", version: v1, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher });
    await mirrorFile(e, { kind: "file", version: v2, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher });
    expect(e.ASSET_MIRROR.objects.size).toBe(1);
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_blobs").first()).toEqual({ n: 1 });
  });

  it("reaches the global fetch without detaching it", async () => {
    const e = env();
    const v = await seed(e, ["https://x.example/a.zip"]);
    const inner = fakeFetch([[/./, () => file(new Uint8Array([1, 2, 3]), "a.zip")]]);
    const real = globalThis.fetch;
    globalThis.fetch = function (this: unknown, input: RequestInfo | URL, init?: RequestInit) {
      if (this !== undefined && this !== globalThis) throw new TypeError("Illegal invocation");
      return inner(input, init);
    } as typeof fetch;
    try {
      await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { hasher: nodeHasher });
    } finally {
      globalThis.fetch = real;
    }
    expect(await e.DB.prepare("SELECT status FROM mod_files").first()).toEqual({ status: "done" });
  });

  it("is idempotent: a duplicate delivery does nothing", async () => {
    const e = env();
    const v = await seed(e, ["https://x.example/a.zip"]);
    const f = fakeFetch([[/./, () => file(new Uint8Array([1, 2, 3]), "a.zip")]]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher });
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher });
    expect(f.calls).toHaveLength(1);
  });

  it("expands a MediaFire folder into parts, then fetches each part on its own", async () => {
    const e = env();
    const v = await seed(e, ["https://www.mediafire.com/folder/z47z7eaf2rmm3/publicyz450"]);
    const f = fakeFetch([
      [/content_type=files/, () => new Response(fixture("mediafire-folder-files.json"))],
      [/content_type=folders/, () => new Response(fixture("mediafire-folder-folders.json"))],
      [/www\.mediafire\.com\/file\/6hilor1pbmga286/, () => new Response(fixture("mediafire-file.html"), { headers: { "content-type": "text/html" } })],
      [/download850\.mediafire\.com/, () => file(new Uint8Array([0, 0, 2, 0]), "metals.tga")],
    ]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher, now: 9 });
    const parts = await e.DB.prepare("SELECT part, rel, status FROM mod_files WHERE version_id = ? ORDER BY part").bind(v).all();
    expect(parts.results).toEqual([
      { part: 0, rel: null, status: "folder" },
      { part: 1, rel: "publicyz450/metals.tga", status: "pending" },
      { part: 2, rel: "publicyz450/metals_n.tga", status: "pending" },
      { part: 3, rel: "publicyz450/plastics.tga", status: "pending" },
      { part: 4, rel: "publicyz450/plastics_n.tga", status: "pending" },
    ]);
    await e.DB.prepare("UPDATE mod_files SET status = 'queued' WHERE part = 1").run();
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 1 }, { fetch: f, hasher: nodeHasher });
    expect(await e.DB.prepare("SELECT status, filename FROM mod_files WHERE part = 1").first()).toEqual({
      status: "done",
      filename: "metals.tga",
    });
  });

  it("downloads and decrypts a MEGA file into a plain blob", async () => {
    const e = env();
    const raw = new Uint8Array(32).map((_, i) => i * 7);
    const { key, nonce } = megaFileKey(raw);
    const plain = new TextEncoder().encode("PK\u0003\u0004 a pack from MEGA");
    const counter = new Uint8Array(16);
    counter.set(nonce);
    const k = await crypto.subtle.importKey("raw", key, "AES-CTR", false, ["encrypt"]);
    const cipher = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-CTR", counter, length: 64 }, k, plain));
    // The attribute blob: `MEGA{"n":"Pack.zip"}` zero-padded, AES-CBC, zero IV, no padding.
    const attrPlain = new TextEncoder().encode('MEGA{"n":"Pack.zip"}');
    const padded = new Uint8Array(32);
    padded.set(attrPlain);
    const cbc = await crypto.subtle.importKey("raw", key, "AES-CBC", false, ["encrypt"]);
    const at = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-CBC", iv: new Uint8Array(16) }, cbc, padded)).slice(0, 32);
    const b64 = (b: Uint8Array) => btoa(String.fromCharCode(...b)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

    const v = await seed(e, [`https://mega.nz/file/HANDLE01#${b64(raw)}`]);
    const f = fakeFetch([
      [/g\.api\.mega\.co\.nz/, () => Response.json([{ s: plain.length, at: b64(at), g: "https://gfs.example/dl/1" }])],
      [/gfs\.example/, () => new Response(cipher)],
    ]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher });
    const sha = await sha256(plain);
    expect(await e.DB.prepare("SELECT status, sha256, filename FROM mod_files").first()).toEqual({
      status: "done",
      sha256: sha,
      filename: "Pack.zip",
    });
    expect(e.ASSET_MIRROR.objects.get(`liveries/${sha}`)!.bytes).toEqual(plain);
  });

  it("fails a dead link at once, retries a busy host later, and hands OneDrive to the runner", async () => {
    const e = env();
    const v = await seed(e, ["https://x.example/gone.zip", "https://y.example/busy.zip", "https://1drv.ms/u/s!abc"]);
    const f = fakeFetch([
      [/gone/, () => new Response("", { status: 404 })],
      [/busy/, () => new Response("", { status: 503 })],
      [/onedrive/, () => Response.json({ error: { code: "itemNotFound" } })],
    ]);
    for (const idx of [0, 1, 2]) await mirrorFile(e, { kind: "file", version: v, idx, part: 0 }, { fetch: f, hasher: nodeHasher, now: 100 });
    const { results } = await e.DB.prepare("SELECT idx, status, attempts FROM mod_files ORDER BY idx").all();
    expect(results).toEqual([
      { idx: 0, status: "failed", attempts: 1 },
      { idx: 1, status: "retry", attempts: 1 },
      { idx: 2, status: "runner", attempts: 0 },
    ]);
  });

  it("leaves a file over the Worker's cap to the runner without reading it", async () => {
    const e = env();
    const v = await seed(e, ["https://x.example/huge.pkz"]);
    const f = fakeFetch([
      [/huge/, () => new Response("x", { headers: { "content-type": "application/octet-stream", "content-length": String(3 * 1024 ** 3) } })],
    ]);
    await mirrorFile(e, { kind: "file", version: v, idx: 0, part: 0 }, { fetch: f, hasher: nodeHasher });
    expect(await e.DB.prepare("SELECT status FROM mod_files").first()).toEqual({ status: "runner" });
  });
});
