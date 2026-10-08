/**
 * A post's own words and pictures, and the queued page backfill, against the whole recorded
 * page of https://mxb-mods.com/careless-beta/ (`test/fixtures/mods/careless-beta.full.html`,
 * recorded 2026-10-08): one YouTube video, one line of text, five screenshots.
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  cooldownMs,
  dispatchPages,
  PAGE_LANES,
  PAGE_REV,
  PAGE_SPACING_MS,
  readPageJobs,
  retryAfterMs,
  runMirror,
  upsertPost,
  type Category,
  type MirrorJob,
} from "../../src/mirror";
import { consumeMirror } from "../../src/mirrorfetch";
import { contentRegion, parseBody, parsePostBody, safeBlocks } from "../../src/modbody";
import { publicModRoutes } from "../../src/modapi";
import { evictUnused } from "../../src/mirrorpolicy";
import { d1 } from "../../test/d1sqlite";
import { drainPages, fakeBucket, fakeFetch, fakeQueue } from "../../test/modfakes";

const FIX = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "test", "fixtures", "mods");
const CARELESS = readFileSync(join(FIX, "careless-beta.full.html"), "utf8");

const SHOTS = [
  "https://mxb-mods.com/wp-content/uploads/2026/10/Careless1-1280x720.webp",
  "https://mxb-mods.com/wp-content/uploads/2026/10/careless2-1280x720.webp",
  "https://mxb-mods.com/wp-content/uploads/2026/10/careless3-1280x720.webp",
  "https://mxb-mods.com/wp-content/uploads/2026/10/careless4-1280x720.webp",
  "https://mxb-mods.com/wp-content/uploads/2026/10/careless5-1280x720.webp",
];

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

/** A WebP whose bytes differ per address, so each picture is its own blob. */
function webp(tag: string): Uint8Array {
  const enc = new TextEncoder();
  return new Uint8Array([...enc.encode("RIFF"), 8, 0, 0, 0, ...enc.encode("WEBPVP8 "), ...enc.encode(tag)]);
}

function env(extra: Record<string, unknown> = {}) {
  return {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue<MirrorJob>(),
    MXB_MIRROR: "on",
    MXB_ASSETS_CDN: "https://cdn.mxbsecure.com",
    ...extra,
  } as unknown as Env & { ASSET_MIRROR: ReturnType<typeof fakeBucket>; MIRROR_QUEUE: ReturnType<typeof fakeQueue<MirrorJob>> };
}

function site(extra: [RegExp, (req: Request) => Response | Promise<Response>][] = []) {
  return fakeFetch([
    ...extra,
    [/\/robots\.txt$/, () => new Response("User-agent: *\nAllow: /ads.txt\n")],
    [/\/wp-json\/wp\/v2\/categories/, () => Response.json([...TREE.values()])],
    [/orderby=modified/, () => Response.json([POST])],
    [/orderby=id/, () => Response.json([{ id: POST.id }])],
    [/careless-beta\/$/, () => new Response(CARELESS, { headers: { "content-type": "text/html" } })],
    [/\.webp$/, (req) => new Response(webp(req.url), { headers: { "content-type": "image/webp" } })],
  ]);
}

const get = (e: Env, path: string) =>
  publicModRoutes(new Request(`https://api.mxbsecure.com${path}`), new URL(`https://api.mxbsecure.com${path}`), e);

describe("the recorded post's words and pictures", () => {
  it("extracts every content image, in page order, at its 1280 size", () => {
    const { images } = parsePostBody(CARELESS);
    expect(images.map((i) => i.src)).toEqual(SHOTS);
    expect(images.every((i) => i.width === 1280 && i.height === 720)).toBe(true);
  });

  it("takes nothing from outside the post: header, logos, sidebar, related posts", () => {
    // The page has many more pictures than the post does.
    expect((CARELESS.match(/<img\b/g) ?? []).length).toBeGreaterThan(20);
    const region = contentRegion(CARELESS);
    expect(region).not.toContain("download-container");
    expect(region).not.toContain("Track Info");
    expect(parsePostBody(CARELESS).images.some((i) => /Logo|banner|carelessmain/i.test(i.src))).toBe(false);
  });

  it("keeps the description's text and video, and drops the theme's heading and jump link", () => {
    const { blocks } = parsePostBody(CARELESS);
    expect(blocks).toEqual([
      { t: "video", yt: "vrLnTy5RW6Q" },
      { t: "p", c: [{ x: "Will be adding more assets soon. I hope you enjoy!" }] },
    ]);
  });
});

describe("description sanitising", () => {
  const hostile = `
    <h1>Description</h1>
    <p onclick="steal()">Hi <b>bold <i>both</i></b> <a href="javascript:alert(1)">bad</a> <a href="https://ok.example/x">good</a></p>
    <script>document.cookie</script><style>p{}</style>
    <img src="x" onerror="alert(1)"><img src="https://evil.example/a.png">
    <img src="data:image/png;base64,AAAA">
    <iframe src="https://evil.example/frame"></iframe>
    <svg/><p>after svg</p>
    <ul><li>one</li><li>two<br>lines</li></ul>
    <h3>Install</h3><p>&lt;script&gt; is text</p>`;

  it("keeps text, three marks and http(s) links, and nothing else", () => {
    const { blocks, images } = parseBody(hostile);
    expect(images).toEqual([]);
    const json = JSON.stringify(blocks);
    expect(json).not.toMatch(/javascript:|onerror|onclick|document\.cookie|evil\.example|p\{\}/);
    expect(blocks[0]).toEqual({
      t: "p",
      c: [
        { x: "Hi " },
        { x: "bold ", b: 1 },
        { x: "both", b: 1, i: 1 },
        { x: " bad " },
        { x: "good", a: "https://ok.example/x" },
      ],
    });
    expect(blocks).toContainEqual({ t: "p", c: [{ x: "after svg" }] });
    expect(blocks).toContainEqual({ t: "li", c: [{ x: "two\nlines" }] });
    expect(blocks).toContainEqual({ t: "h", c: [{ x: "Install" }] });
    // An escaped tag is text, and stays text: the site renders it as such.
    expect(blocks).toContainEqual({ t: "p", c: [{ x: "<script> is text" }] });
  });

  it("checks stored blocks again on the way out", () => {
    expect(
      safeBlocks(
        JSON.stringify([
          { t: "p", c: [{ x: "ok", a: "javascript:alert(1)" }, { x: "<b>raw</b>", onclick: "x" }] },
          { t: "script", c: [{ x: "nope" }] },
          { t: "video", yt: "not an id at all" },
          { t: "video", yt: "vrLnTy5RW6Q" },
          "string",
        ]),
      ),
    ).toEqual([
      { t: "p", c: [{ x: "ok" }, { x: "<b>raw</b>" }] },
      { t: "video", yt: "vrLnTy5RW6Q" },
    ]);
    expect(safeBlocks("not json")).toEqual([]);
  });

  it("copies pictures only from the site and the hosts its authors use, and no icons", () => {
    const { images } = parseBody(`
      <img src="http://mxb-mods.com/wp-content/uploads/a.jpg">
      <img src="/wp-content/uploads/b.jpg">
      <img src="https://github.com/user-attachments/assets/c">
      <img src="https://tracker.example/pixel.gif">
      <img src="https://mxb-mods.com/wp-content/uploads/icon.png" width="16" height="16">
      <img src="https://mxb-mods.com/wp-includes/images/smilies/x.png">`);
    expect(images.map((i) => i.src)).toEqual([
      "https://mxb-mods.com/wp-content/uploads/a.jpg",
      "https://mxb-mods.com/wp-content/uploads/b.jpg",
      "https://github.com/user-attachments/assets/c",
    ]);
  });

  it("keeps at most twelve pictures, and one of each", () => {
    const tags = Array.from({ length: 20 }, (_, i) => `<img src="https://mxb-mods.com/wp-content/uploads/p${i}.jpg">`);
    tags.push(`<img src="https://mxb-mods.com/wp-content/uploads/p0-640x360.jpg">`);
    expect(parseBody(tags.join("")).images).toHaveLength(12);
    expect(parseBody(tags[0] + tags[20]).images).toHaveLength(1);
  });
});

describe("the post read through the queue", () => {
  it("stores the pictures as img/<sha256>.<ext> and serves them with the description", async () => {
    const e = env();
    const f = site();
    await runMirror(e, { now: 10_000, fetch: f, wait: async () => {} });
    await drainPages(e, { now: 10_000, fetch: f, wait: async () => {} });

    const rows = await e.DB.prepare("SELECT idx, src, width, height FROM mod_asset_images ORDER BY idx").all();
    expect(rows.results).toEqual(SHOTS.map((src, idx) => ({ idx, src, width: 1280, height: 720 })));
    const keys = [...e.ASSET_MIRROR.objects.keys()].filter((k) => k.startsWith("img/"));
    expect(keys).toHaveLength(5);
    expect(keys.every((k) => /^img\/[0-9a-f]{64}\.webp$/.test(k))).toBe(true);
    expect(e.ASSET_MIRROR.objects.get(keys[0])!.httpMetadata).toMatchObject({ contentType: "image/webp" });

    const a = await e.DB.prepare("SELECT public_id, page_rev, page_read_at FROM mod_assets").first<{ public_id: string; page_rev: number; page_read_at: number }>();
    expect(a!.page_rev).toBe(PAGE_REV);
    expect(a!.page_read_at).toBeGreaterThan(0);
    const body = (await (await get(e, `/v1/assets/${a!.public_id}`))!.json()) as {
      images: { url: string; width: number; height: number }[];
      body: unknown[];
    };
    expect(body.images).toHaveLength(5);
    expect(body.images.every((i) => /^https:\/\/cdn\.mxbsecure\.com\/img\/[0-9a-f]{64}\.webp$/.test(i.url))).toBe(true);
    expect(body.images[0]).toMatchObject({ width: 1280, height: 720 });
    expect(body.body).toEqual([
      { t: "video", yt: "vrLnTy5RW6Q" },
      { t: "p", c: [{ x: "Will be adding more assets soon. I hope you enjoy!" }] },
    ]);

    // The pictures stay however long nobody downloads the mod's files.
    await evictUnused(e, 10_000 + 400 * 86_400_000);
    expect([...e.ASSET_MIRROR.objects.keys()].filter((k) => k.startsWith("img/"))).toHaveLength(5);
  });

  it("re-reads rows indexed before pictures, after new pages, without fetching pictures twice", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 0);
    await upsertPost(e, { ...POST, id: 2, slug: "new-one", link: "https://mxb-mods.com/new-one/" }, TREE, 0);
    // The careless row was read before this change: ok, at parser 0.
    await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok', page_rev = 0 WHERE source_ref = ?").bind(POST.id).run();
    expect(await dispatchPages(e, 1000)).toBe(2);
    const order = e.MIRROR_QUEUE.sent.map((j) => (j as { id: number }).id);
    const ids = await e.DB.prepare("SELECT id, source_ref FROM mod_assets").all<{ id: number; source_ref: number }>();
    const idOf = (ref: number) => ids.results.find((r) => r.source_ref === ref)!.id;
    expect(order).toEqual([idOf(2), idOf(POST.id)]);

    const f = site();
    await drainPages(e, { now: 2000, fetch: f, wait: async () => {} });
    const shot = (c: string) => SHOTS.some((u) => c.endsWith(u));
    expect(f.calls.filter(shot)).toHaveLength(5);
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_asset_images").first()).toEqual({ n: 5 });

    // A later parser re-reads it again: same pictures, nothing fetched but the page.
    await e.DB.prepare("UPDATE mod_assets SET page_rev = 0 WHERE source_ref = ?").bind(POST.id).run();
    expect(await dispatchPages(e, 3000)).toBe(1);
    const g = site();
    await drainPages(e, { now: 3000, fetch: g, wait: async () => {} });
    expect(g.calls.filter((c) => c.endsWith(".webp"))).toEqual([]);
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_asset_images").first()).toEqual({ n: 5 });
  });

  it("skips pictures over 3 MB, and anything that isn't one by its bytes", async () => {
    const e = env();
    const big = new Uint8Array(3 * 1024 * 1024 + 1);
    big.set(webp("big"));
    const f = site([
      [/Careless1-1280x720\.webp$/, () => new Response(big, { headers: { "content-type": "image/webp" } })],
      // No length header: cut off once past the cap.
      [/careless2-1280x720\.webp$/, () => new Response(new Blob([big]).stream(), { headers: { "content-type": "image/webp" } })],
      [/careless3-1280x720\.webp$/, () => new Response("<html>not a picture</html>", { headers: { "content-type": "image/webp" } })],
      [/careless4-1280x720\.webp$/, () => new Response('<svg xmlns="http://www.w3.org/2000/svg"/>', { headers: { "content-type": "image/svg+xml" } })],
    ]);
    await upsertPost(e, POST, TREE, 0);
    await dispatchPages(e, 1000);
    await drainPages(e, { now: 1000, fetch: f, wait: async () => {} });
    const rows = await e.DB.prepare("SELECT src FROM mod_asset_images ORDER BY idx").all<{ src: string }>();
    expect(rows.results.map((r) => r.src)).toEqual([SHOTS[4]]);
    expect(await e.DB.prepare("SELECT page_status FROM mod_assets").first()).toEqual({ page_status: "ok" });
  });
});

describe("the backfill", () => {
  async function many(e: Env, n: number) {
    for (let i = 1; i <= n; i++) {
      await upsertPost(e, { ...POST, id: i, slug: `p${i}`, link: `https://mxb-mods.com/p${i}/` }, TREE, 0);
    }
  }

  it("leases hundreds of pages in one go: one JSON parameter, sends of at most 100", async () => {
    const sends: number[] = [];
    const q = fakeQueue<MirrorJob>();
    const counting = {
      ...q,
      sendBatch: async (msgs: { body: MirrorJob }[]) => {
        sends.push(msgs.length);
        await q.sendBatch(msgs);
      },
    };
    const e = env({ MIRROR_QUEUE: counting });
    await many(e, 250);
    expect(await dispatchPages(e, 1000, 1000)).toBe(250);
    expect(sends).toEqual([100, 100, 50]);
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_assets WHERE page_status = 'queued'").first()).toEqual({ n: 250 });
    // Topped up, not resent, while the leases hold; sent again once they run out.
    expect(await dispatchPages(e, 2000, 1000)).toBe(0);
    expect(await dispatchPages(e, 1000 + 61 * 60_000, 1000)).toBe(250);
  });

  it("tops the queue up only to its target", async () => {
    const e = env();
    await many(e, 30);
    expect(await dispatchPages(e, 1000, 10)).toBe(10);
    expect(await dispatchPages(e, 1000, 10)).toBe(0);
    expect(await dispatchPages(e, 1000, 25)).toBe(15);
  });

  it(`reads a batch on ${PAGE_LANES} lanes at most, each spaced`, async () => {
    const e = env();
    await many(e, 12);
    await dispatchPages(e, 1000);
    let open = 0;
    let peak = 0;
    const page = CARELESS.replace(/<img\b[^>]*>/g, "");
    const f = fakeFetch([
      [/\/robots\.txt$/, () => new Response("")],
      [
        /\/p\d+\/$/,
        async () => {
          open++;
          peak = Math.max(peak, open);
          await new Promise((r) => setTimeout(r, 5));
          open--;
          return new Response(page, { headers: { "content-type": "text/html" } });
        },
      ],
    ]);
    const waits: number[] = [];
    const r = await drainPages(e, { now: 1000, fetch: f, wait: async (ms) => void waits.push(ms) });
    expect(r).toMatchObject({ read: 12, deferred: 0 });
    expect(peak).toBe(PAGE_LANES);
    // Every request to the site, a lane's first included, waits at least 3 s.
    const sent = f.calls.filter((c) => !c.endsWith("/robots.txt"));
    expect(waits).toHaveLength(sent.length);
    expect(waits.every((w) => w >= PAGE_SPACING_MS && w >= 3000)).toBe(true);
  });

  it("stops every lane on a 429, honours Retry-After, and puts the rest back for after it", async () => {
    const e = env();
    await many(e, 8);
    await dispatchPages(e, 1000);
    const f = fakeFetch([
      [/\/robots\.txt$/, () => new Response("")],
      [/\/p\d+\/$/, () => new Response("slow down", { status: 429, headers: { "retry-after": "3600" } })],
    ]);
    const r = await readPageJobs(e, e.MIRROR_QUEUE.sent.map((j) => (j as { id: number }).id), {
      now: 1000,
      clock: () => 1000,
      fetch: f,
      wait: async () => {},
    });
    expect(r.read).toBe(0);
    expect(r.deferred).toBe(8);
    // Only what was already in flight when the first refusal came: no lane asks again.
    expect(f.calls.filter((c) => /\/p\d+\/$/.test(c)).length).toBeLessThanOrEqual(PAGE_LANES);
    const st = await e.DB.prepare("SELECT value FROM mirror_state WHERE key = 'cooldown'").first<{ value: string }>();
    expect(JSON.parse(st!.value)).toMatchObject({ until: 1000 + 3600_000, status: 429 });
    const rows = await e.DB.prepare("SELECT DISTINCT page_status, page_due_at, page_attempts FROM mod_assets").all();
    expect(rows.results).toEqual([{ page_status: "due", page_due_at: 1000 + 3600_000, page_attempts: 0 }]);

    // The cron leaves the site alone meanwhile, and dispatches no pages.
    const g = site();
    e.MIRROR_QUEUE.sent.length = 0;
    await runMirror(e, { now: 2000, fetch: g, wait: async () => {} });
    expect(g.calls).toEqual([]);
    expect(e.MIRROR_QUEUE.sent).toEqual([]);
  });

  it("a batch that arrives during a cooldown reads nothing", async () => {
    const e = env();
    await many(e, 3);
    await dispatchPages(e, 1000);
    await e.DB.prepare("INSERT INTO mirror_state (key, value) VALUES ('cooldown', ?)").bind(JSON.stringify({ until: 9000, status: 403 })).run();
    const f = site();
    const r = await drainPages(e, { now: 1000, clock: () => 1000, fetch: f, wait: async () => {} });
    expect(r).toMatchObject({ read: 0, deferred: 3 });
    expect(f.calls).toEqual([]);
  });

  it("backs off longer while refusals keep coming", () => {
    expect(cooldownMs(0, 0)).toBe(10 * 60_000);
    expect(cooldownMs(1, 0)).toBe(20 * 60_000);
    expect(cooldownMs(9, 0)).toBe(2 * 3600_000);
    expect(cooldownMs(0, 5 * 3600_000)).toBe(5 * 3600_000);
    expect(retryAfterMs("120", 0)).toBe(120_000);
    expect(retryAfterMs(new Date(60_000).toUTCString(), 0)).toBe(60_000);
    expect(retryAfterMs("soon", 0)).toBe(0);
  });

  it("walks four listing pages a run and resumes where it stopped", async () => {
    const e = env();
    const posts = Array.from({ length: 300 }, (_, i) => ({
      ...POST,
      id: 1000 + i,
      slug: `w${i}`,
      link: `https://mxb-mods.com/w${i}/`,
      modified: `2026-10-07T19:${String(59 - Math.floor(i / 60)).padStart(2, "0")}:${String(59 - (i % 60)).padStart(2, "0")}`,
    }));
    const f = fakeFetch([
      [/\/robots\.txt$/, () => new Response("")],
      [/\/wp-json\/wp\/v2\/categories/, () => Response.json([...TREE.values()])],
      [
        /orderby=modified/,
        (req) => {
          const offset = Number(new URL(req.url).searchParams.get("offset") ?? 0);
          return Response.json(posts.slice(offset, offset + 50));
        },
      ],
      [/orderby=id/, () => Response.json([])],
    ]);
    await runMirror(e, { now: 1000, fetch: f, wait: async () => {} });
    // Offsets 0, 45, 90, 135 (pages overlap by five): 185 posts.
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_assets").first()).toEqual({ n: 185 });
    expect(f.calls.filter((c) => c.includes("orderby=modified"))).toHaveLength(4);
    // The queue is only topped up to its small target.
    expect(e.MIRROR_QUEUE.sent.filter((j) => j.kind === "page")).toHaveLength(60);
    await runMirror(e, { now: 1000 + 600_000, fetch: f, wait: async () => {} });
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_assets").first()).toEqual({ n: 300 });
  });

  it("the consumer reads a batch's pages and still runs its files, acking all", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 0);
    await dispatchPages(e, 1000);
    const jobs: MirrorJob[] = [...e.MIRROR_QUEUE.sent, { kind: "file", version: 99, idx: 0, part: 0 }];
    e.MIRROR_QUEUE.sent.length = 0;
    const acked: MirrorJob[] = [];
    const batch = {
      queue: "mxb-mirror",
      messages: jobs.map((body) => ({ body, ack: () => void acked.push(body), retry: () => {} })),
    } as unknown as MessageBatch<MirrorJob>;
    await consumeMirror(batch, e, { now: 1000, fetch: site(), wait: async () => {} });
    expect(acked).toHaveLength(2);
    expect(await e.DB.prepare("SELECT page_status FROM mod_assets").first()).toEqual({ page_status: "ok" });
    expect(await e.DB.prepare("SELECT COUNT(*) AS n FROM mod_asset_images").first()).toEqual({ n: 5 });
  });
});

describe("counts per type", () => {
  it("answers the total and each type from one query", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 0);
    await upsertPost(e, { ...POST, id: 2, slug: "x", categories: [] }, TREE, 0);
    await upsertPost(e, { ...POST, id: 3, slug: "y", categories: [] }, TREE, 0);
    expect(await (await get(e, "/v1/assets/stats"))!.json()).toEqual({
      total: 3,
      by_type: { paints: 0, bikes: 0, liveries: 0, kits: 0, tracks: 1, other: 2 },
    });
  });
});
