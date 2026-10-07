import { describe, expect, it } from "vitest";
import { getAsset, publicModRoutes, searchAssets } from "../src/modapi";
import { moderate, moderationQueue, reportAsset } from "../src/modreports";
import {
  QUOTA,
  completeUpload,
  deleteMod,
  editMod,
  myMods,
  openUpload,
  parseListParts,
  parseOpen,
  uploadStatus,
  verifyUpload,
  type Uploader,
} from "../src/uploads";
import { addAccount, d1 } from "./d1sqlite";
import { fakeBucket, fakeFetch, fakePe, fakeQueue, makeZip, nodeHasher, sha256 } from "./modfakes";

const RIDER: Uploader = { id: "acc-rider", rider_name: "Frosty", steam_id: "steam-rider" };
const OTHER: Uploader = { id: "acc-other", rider_name: "Other", steam_id: "steam-other" };

async function env() {
  const e = {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue(),
    MXB_ASSETS_CDN: "https://cdn.mxbsecure.com",
    R2_ACCESS_KEY_ID: "test-access-key",
    R2_SECRET_ACCESS_KEY: "test-secret-key",
    R2_S3_ENDPOINT: "https://r2.example.invalid",
  } as unknown as Env & {
    ASSET_MIRROR: ReturnType<typeof fakeBucket>;
    ASSET_LOCKED: ReturnType<typeof fakeBucket>;
    MIRROR_QUEUE: ReturnType<typeof fakeQueue>;
  };
  await addAccount(e.DB, RIDER.id, RIDER.rider_name);
  await addAccount(e.DB, OTHER.id, OTHER.rider_name);
  return e;
}

const post = (body: unknown) => new Request("https://api.mxbsecure.com/v1/uploads", { method: "POST", body: JSON.stringify(body) });

/** Open a session, write the bytes the way the app would (straight to R2), and complete it. */
async function upload(e: Awaited<ReturnType<typeof env>>, bytes: Uint8Array, meta: Record<string, unknown> = {}, who = RIDER, declaredSha?: string) {
  const sha = declaredSha ?? (await sha256(bytes));
  const opened = await openUpload(
    post({ filename: "Red Bull KTM.zip", size: bytes.length, sha256: sha, type: "liveries", title: "Red Bull KTM", bike: "KTM", ...meta }),
    who,
    e,
    1000,
  );
  expect(opened.status).toBe(201);
  const body = opened.body as { id: string; part_size: number; parts: { part: number; url: string }[] };
  const row = await e.DB.prepare("SELECT r2_key, r2_upload_id FROM mod_uploads WHERE id = ?").bind(body.id).first<{ r2_key: string; r2_upload_id: string }>();
  const mp = e.ASSET_LOCKED.resumeMultipartUpload(row!.r2_key, row!.r2_upload_id);
  const parts = [];
  for (const p of body.parts) {
    const slice = bytes.subarray((p.part - 1) * body.part_size, p.part * body.part_size);
    const up = await mp.uploadPart(p.part, slice);
    parts.push({ part: p.part, etag: up.etag });
  }
  const done = await completeUpload(new Request("https://x", { method: "POST", body: JSON.stringify({ parts }) }), body.id, who, e, 2000);
  return { id: body.id, opened: body, done };
}

describe("opening an upload", () => {
  it("validates what the app sends", () => {
    expect(parseOpen({ filename: "x.exe", size: 1, sha256: "a".repeat(64), title: "t", type: "kits" })).toMatch(/only \.pkz/);
    expect(parseOpen({ filename: "x.zip", size: 3 * 1024 ** 3, sha256: "a".repeat(64), title: "t", type: "kits" })).toMatch(/limit/);
    expect(parseOpen({ filename: "x.pnt", size: 65 * 1024 ** 2, sha256: "a".repeat(64), title: "t", type: "kits" })).toMatch(/limit/);
    expect(parseOpen({ filename: "x.zip", size: 1, sha256: "nope", title: "t", type: "kits" })).toMatch(/sha256/);
    expect(parseOpen({ filename: "x.zip", size: 1, sha256: "a".repeat(64), type: "kits" })).toMatch(/title/);
    expect(parseOpen({ filename: "x.zip", size: 1, sha256: "a".repeat(64), title: "t", type: "boats" })).toMatch(/type/);
    expect(parseOpen({ filename: "x.zip", size: 1, sha256: "a".repeat(64), title: "t", type: "kits", visibility: "secret" })).toMatch(/visibility/);
    expect(parseOpen({ filename: "a/b\\c.zip", size: 1, sha256: "A".repeat(64), title: "t", type: "kits" })).toMatchObject({
      filename: "a_b_c.zip",
      sha256: "a".repeat(64),
      kind: "zip",
    });
  });

  it("needs a Steam-confirmed account and configured credentials", async () => {
    const e = await env();
    const body = { filename: "x.zip", size: 10, sha256: "a".repeat(64), title: "t", type: "kits" };
    expect((await openUpload(post(body), { ...RIDER, steam_id: null }, e)).status).toBe(403);
    const bare = { ...e, R2_S3_ENDPOINT: undefined } as unknown as Env;
    expect((await openUpload(post(body), RIDER, bare)).status).toBe(503);
  });

  it("hands out presigned part URLs against the private bucket", async () => {
    const e = await env();
    const r = await openUpload(post({ filename: "x.zip", size: 70 * 1024 * 1024, sha256: "a".repeat(64), title: "t", type: "kits" }), RIDER, e, 0);
    const body = r.body as { parts: { part: number; url: string }[]; part_size: number };
    expect(body.parts.map((p) => p.part)).toEqual([1, 2, 3]);
    const u = new URL(body.parts[1].url);
    expect(u.origin + u.pathname).toMatch(/^https:\/\/r2\.example\.invalid\/mxb-private\/quarantine\/[0-9a-f]{32}$/);
    expect(u.searchParams.get("partNumber")).toBe("2");
    expect(u.searchParams.get("uploadId")).toBe("up1");
    expect(u.searchParams.get("X-Amz-Signature")).toMatch(/^[0-9a-f]{64}$/);
    expect(u.searchParams.get("X-Amz-Expires")).toBe("21600");
  });

  it("enforces the open-session quota", async () => {
    const e = await env();
    const body = { filename: "x.zip", size: 10, sha256: "a".repeat(64), title: "t", type: "kits" };
    for (let i = 0; i < QUOTA.openSessions; i++) expect((await openUpload(post(body), RIDER, e, 0)).status).toBe(201);
    const r = await openUpload(post(body), RIDER, e, 0);
    expect(r.status).toBe(429);
    expect(JSON.stringify(r.body)).toMatch(/at once/);
  });

  it("resumes: lists what R2 holds and re-signs only the missing parts", async () => {
    const e = await env();
    const r = await openUpload(post({ filename: "x.zip", size: 70 * 1024 * 1024, sha256: "a".repeat(64), title: "t", type: "kits" }), RIDER, e, 0);
    const id = (r.body as { id: string }).id;
    const listed = `<ListPartsResult><Part><PartNumber>1</PartNumber><ETag>&quot;e1&quot;</ETag><Size>33554432</Size></Part></ListPartsResult>`;
    const f = fakeFetch([[/uploadId=up1/, () => new Response(listed)]]);
    const s = await uploadStatus(id, RIDER, e, 1, f);
    const body = s.body as { uploaded: { part: number }[]; parts: { part: number }[] };
    expect(body.uploaded).toEqual([{ part: 1, etag: '"e1"', size: 33554432 }]);
    expect(body.parts.map((p) => p.part)).toEqual([2, 3]);
    expect((await uploadStatus(id, OTHER, e, 1, f)).status).toBe(404);
    expect(parseListParts("<x/>")).toEqual([]);
  });
});

describe("checking an upload", () => {
  it("quarantines, checks, then publishes under the type's prefix", async () => {
    const e = await env();
    const zip = await makeZip([{ name: "paints/RedBull.pnt", data: "PNT\0paint" }]);
    const { id, done } = await upload(e, zip);
    expect(done).toMatchObject({ status: 202, body: { state: "verifying" } });
    expect(e.MIRROR_QUEUE.sent).toEqual([{ kind: "upload", id }]);
    expect(await e.DB.prepare("SELECT state FROM mod_versions").first()).toEqual({ state: "quarantine" });
    // Not listed while in quarantine.
    expect((await searchAssets(new URL("https://x/v1/assets/search"), e)).body).toMatchObject({ total: 0 });

    await verifyUpload(e, id, { hasher: nodeHasher, now: 3000 });
    const sha = await sha256(zip);
    expect(await e.DB.prepare("SELECT state FROM mod_uploads").first()).toEqual({ state: "live" });
    expect(await e.DB.prepare("SELECT state FROM mod_versions").first()).toEqual({ state: "live" });
    expect([...e.ASSET_MIRROR.objects.keys()]).toEqual([`liveries/${sha}`]);
    expect([...e.ASSET_LOCKED.objects.keys()]).toEqual([]);

    const found = (await searchAssets(new URL("https://x/v1/assets/search?q=red+bull"), e)).body as { results: { id: number; author: string; source: string }[] };
    expect(found.results).toHaveLength(1);
    expect(found.results[0]).toMatchObject({ author: "Frosty", source: "upload" });
  });

  it("rejects a file whose hash isn't the one declared, and frees it", async () => {
    const e = await env();
    const zip = await makeZip([{ name: "a.pnt", data: "PNT\0" }]);
    const { id } = await upload(e, zip, {}, RIDER, "b".repeat(64));
    await verifyUpload(e, id, { hasher: nodeHasher });
    expect(await e.DB.prepare("SELECT state, error FROM mod_uploads").first()).toEqual({
      state: "rejected",
      error: "the file's SHA-256 doesn't match the one declared",
    });
    expect(await e.DB.prepare("SELECT state FROM mod_versions").first()).toEqual({ state: "rejected" });
    expect(e.ASSET_LOCKED.objects.size).toBe(0);
    expect(e.ASSET_MIRROR.objects.size).toBe(0);
  });

  it("rejects a program hidden in the archive, and says which file", async () => {
    const e = await env();
    const zip = await makeZip([{ name: "paints/a.pnt", data: "PNT\0" }, { name: "paints/b.pnt", data: fakePe() }]);
    const { id } = await upload(e, zip);
    await verifyUpload(e, id, { hasher: nodeHasher });
    expect(await e.DB.prepare("SELECT state, error FROM mod_uploads").first()).toEqual({
      state: "rejected",
      error: "paints/b.pnt is a Windows program (PE)",
    });
  });

  it("refuses a completion that doesn't add up, before it is queued", async () => {
    const e = await env();
    const opened = await openUpload(post({ filename: "x.zip", size: 999, sha256: "a".repeat(64), title: "t", type: "kits" }), RIDER, e, 0);
    const id = (opened.body as { id: string }).id;
    const row = await e.DB.prepare("SELECT r2_key, r2_upload_id FROM mod_uploads").first<{ r2_key: string; r2_upload_id: string }>();
    const up = await e.ASSET_LOCKED.resumeMultipartUpload(row!.r2_key, row!.r2_upload_id).uploadPart(1, new Uint8Array(10));
    const r = await completeUpload(new Request("https://x", { method: "POST", body: JSON.stringify({ parts: [{ part: 1, etag: up.etag }] }) }), id, RIDER, e, 1);
    expect(r.status).toBe(400);
    expect(e.MIRROR_QUEUE.sent).toEqual([]);
    expect(await e.DB.prepare("SELECT state FROM mod_uploads").first()).toEqual({ state: "rejected" });
  });

  it("a new upload of the same mod is its next version", async () => {
    const e = await env();
    const a = await upload(e, await makeZip([{ name: "a.pnt", data: "PNT\0one" }]));
    await verifyUpload(e, a.id, { hasher: nodeHasher });
    const assetId = (a.done.body as { asset_id: number }).asset_id;
    const b = await upload(e, await makeZip([{ name: "a.pnt", data: "PNT\0two" }]), { asset_id: assetId, version: "v2", title: undefined });
    await verifyUpload(e, b.id, { hasher: nodeHasher });
    expect((await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_assets").first())).toEqual({ n: 1 });
    const got = (await getAsset(assetId, new URL("https://api/x"), e)).body as { version: string; version_seq: number; versions: unknown[] };
    expect(got).toMatchObject({ version: "v2", version_seq: 2 });
    expect(got.versions).toHaveLength(2);
    // Someone else can't add a version to it.
    const r = await openUpload(post({ filename: "x.zip", size: 10, sha256: "a".repeat(64), asset_id: assetId }), OTHER, e);
    expect(r.status).toBe(404);
  });
});

describe("owners, visibility and moderation", () => {
  async function published(e: Awaited<ReturnType<typeof env>>, meta: Record<string, unknown> = {}) {
    const u = await upload(e, await makeZip([{ name: "a.pnt", data: `PNT\0${JSON.stringify(meta)}` }]), meta);
    await verifyUpload(e, u.id, { hasher: nodeHasher });
    return (u.done.body as { asset_id: number }).asset_id;
  }
  const search = async (e: Env, q = "") =>
    ((await searchAssets(new URL(`https://x/v1/assets/search?q=${q}`), e)).body as { total: number }).total;

  it("keeps an unlisted mod out of search but reachable by id", async () => {
    const e = await env();
    const id = await published(e, { visibility: "unlisted" });
    expect(await search(e)).toBe(0);
    expect((await getAsset(id, new URL("https://x"), e)).status).toBe(200);
  });

  it("lets the owner edit and delete, and nobody else", async () => {
    const e = await env();
    const id = await published(e);
    const patch = (body: unknown) => new Request("https://x", { method: "PATCH", body: JSON.stringify(body) });
    expect((await editMod(patch({ title: "Renamed" }), id, OTHER, e)).status).toBe(404);
    expect((await editMod(patch({ title: "Renamed" }), id, RIDER, e)).status).toBe(200);
    expect(await search(e, "renamed")).toBe(1);
    expect((await deleteMod(id, OTHER, e)).status).toBe(404);
    expect((await deleteMod(id, RIDER, e)).status).toBe(200);
    expect(await search(e)).toBe(0);
    expect(e.ASSET_MIRROR.objects.size).toBe(0);
    expect((await myMods(RIDER, e)).body).toMatchObject({ mods: [] });
  });

  it("reports queue up; hide takes a mod out, unhide brings it back, remove frees its files", async () => {
    const e = await env();
    const id = await published(e);
    const report = (reason: string) =>
      reportAsset(new Request("https://x", { method: "POST", headers: { "CF-Connecting-IP": "1.2.3.4" }, body: JSON.stringify({ reason }) }), id, e);
    expect((await report("nonsense")).status).toBe(400);
    expect((await report("stolen")).status).toBe(201);
    const q = (await moderationQueue(e)).body as { reported: { id: number; reports_open: number; reports: { reason: string }[] }[] };
    expect(q.reported).toMatchObject([{ id, reports_open: 1, reports: [{ reason: "stolen" }] }]);

    await moderate(id, "hide", "checking with the author", "steam:admin", e);
    expect(await search(e)).toBe(0);
    expect((await getAsset(id, new URL("https://x"), e)).status).toBe(404);
    expect(await e.DB.prepare("SELECT resolution FROM mod_reports").first()).toEqual({ resolution: "hidden" });
    await moderate(id, "unhide", null, "steam:admin", e);
    expect(await search(e)).toBe(1);
    await moderate(id, "remove", "confirmed stolen", "steam:admin", e);
    expect(await search(e)).toBe(0);
    expect(e.ASSET_MIRROR.objects.size).toBe(0);
    expect((await moderate(id, "unhide", null, "steam:admin", e)).status).toBe(404);
  });

  it("caps reports per address per day", async () => {
    const e = await env();
    const id = await published(e);
    const r = () =>
      reportAsset(new Request("https://x", { method: "POST", headers: { "CF-Connecting-IP": "9.9.9.9" }, body: JSON.stringify({ reason: "broken" }) }), id, e);
    for (let i = 0; i < 20; i++) expect((await r()).status).toBe(201);
    expect((await r()).status).toBe(429);
  });
});

describe("the public routes", () => {
  it("search, detail and a download that redirects to the CDN", async () => {
    const e = await env();
    const zip = await makeZip([{ name: "a.pnt", data: "PNT\0" }]);
    const u = await upload(e, zip, { title: "Monster Energy Kawasaki", bike: "Kawasaki; KX450F" });
    await verifyUpload(e, u.id, { hasher: nodeHasher });
    const id = (u.done.body as { asset_id: number }).asset_id;
    const get = (path: string) => publicModRoutes(new Request(`https://api.mxbsecure.com${path}`), new URL(`https://api.mxbsecure.com${path}`), e);

    const s = await get("/v1/assets/search?q=monst%20kawa&type=liveries&bike=kx450");
    expect(s!.status).toBe(200);
    expect(s!.headers.get("access-control-allow-origin")).toBe("*");
    const body = (await s!.json()) as { results: { id: number; bike: string[] }[] };
    expect(body.results.map((r) => r.id)).toEqual([id]);
    expect(body.results[0].bike).toEqual(["Kawasaki", "KX450F"]);

    expect(((await (await get("/v1/assets/search?type=tracks"))!.json()) as { total: number }).total).toBe(0);
    expect((await get("/v1/assets/search?type=boats"))!.status).toBe(400);

    const d = (await (await get(`/v1/assets/${id}`))!.json()) as { files: { download: string; cdn: string; state: string }[] };
    expect(d.files[0].state).toBe("stored");
    const sha = await sha256(zip);
    expect(d.files[0].cdn).toBe(`https://cdn.mxbsecure.com/liveries/${sha}`);

    const r = await get(`/v1/assets/${id}/download/0`);
    expect(r!.status).toBe(302);
    expect(r!.headers.get("location")).toBe(`https://cdn.mxbsecure.com/liveries/${sha}`);
    expect((await get(`/v1/assets/${id}/download/5`))!.status).toBe(404);
    expect(await get("/v1/assets/status")).toBeNull();
  });

  it("ranks a title match above a description match", async () => {
    const e = await env();
    const a = await upload(e, await makeZip([{ name: "a.pnt", data: "PNT\0a" }]), { title: "Plain helmet", description: "inspired by Anderson" });
    const b = await upload(e, await makeZip([{ name: "a.pnt", data: "PNT\0b" }]), { title: "Anderson replica" });
    await verifyUpload(e, a.id, { hasher: nodeHasher });
    await verifyUpload(e, b.id, { hasher: nodeHasher });
    const r = (await searchAssets(new URL("https://x/v1/assets/search?q=anderson"), e)).body as { results: { title: string }[] };
    expect(r.results.map((x) => x.title)).toEqual(["Anderson replica", "Plain helmet"]);
  });

  it("quotes what a user types, so FTS syntax is just text", async () => {
    const e = await env();
    const r = await searchAssets(new URL('https://x/v1/assets/search?q=NEAR(a b) OR "x*" -y:z'), e);
    expect(r.status).toBe(200);
  });
});
