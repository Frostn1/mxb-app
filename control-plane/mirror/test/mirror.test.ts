import { describe, expect, it } from "vitest";
import {
  backoff,
  bikeTerms,
  classify,
  dispatch,
  mentionsServer,
  minusOneSecond,
  parseAuthor,
  parseDownloads,
  parseVersion,
  robotsAllows,
  robotsRules,
  runMirror,
  upsertPost,
  urlFileName,
  writeMirrorVersion,
  type Category,
} from "../../src/mirror";
import { d1 } from "../../test/d1sqlite";
import { drainPages, fakeBucket, fakeFetch, fakeQueue, fixture } from "../../test/modfakes";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const page = (name: string) =>
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", "..", "test", "fixtures", "mods", name), "utf8");

const TREE = new Map<number, Category>(
  [
    { id: 118, name: "Uploads", parent: 0 },
    { id: 22, name: "Tracks", parent: 118 },
    { id: 301, name: "Intermediate", parent: 22 },
    { id: 29, name: "Bikes", parent: 118 },
    { id: 37, name: "Liveries", parent: 29 },
    { id: 45, name: "New Bikes", parent: 29 },
    { id: 30, name: "Rider", parent: 118 },
    { id: 35, name: "Rider Kit", parent: 30 },
    { id: 33, name: "Helmets", parent: 30 },
    { id: 40, name: "Miscellaneous", parent: 118 },
    { id: 174, name: "ReShade Presets", parent: 40 },
    { id: 319, name: "Manufacturers", parent: 0 },
    { id: 102, name: "KTM", parent: 319 },
    { id: 147, name: "Discontinued", parent: 0 },
    { id: 108, name: "2021 KTM 450 SX-F OEM", parent: 147 },
  ].map((c) => [c.id, c]),
);

function env(extra: Record<string, unknown> = {}) {
  return {
    DB: d1(),
    ASSET_MIRROR: fakeBucket(),
    ASSET_LOCKED: fakeBucket(),
    MIRROR_QUEUE: fakeQueue(),
    MXB_MIRROR: "on",
    ...extra,
  } as unknown as Env & { MIRROR_QUEUE: ReturnType<typeof fakeQueue> };
}

describe("classification", () => {
  it("files a post under the most specific type its categories reach", () => {
    expect(classify([301, 102], TREE)).toBe("tracks");
    expect(classify([37, 102, 108], TREE)).toBe("liveries");
    expect(classify([35], TREE)).toBe("kits");
    expect(classify([33], TREE)).toBe("paints");
    expect(classify([45], TREE)).toBe("bikes");
    expect(classify([174], TREE)).toBe("other");
    expect(classify([], TREE)).toBe("other");
  });
  it("collects bike and manufacturer terms", () => {
    expect(bikeTerms([37, 102, 108], TREE)).toEqual(["KTM", "2021 KTM 450 SX-F OEM"]);
  });
});

describe("page parsing (the app's mxb.rs, ported)", () => {
  it("reads the recorded page: downloads, version, byline", () => {
    const html = page("careless-beta.html");
    expect(parseDownloads(html)).toEqual([
      {
        url: "https://drive.google.com/file/d/1R52G084B5MWD-M3J5PmTcYLtiQb2kM_V/view?usp=sharing",
        host: "drive.google.com",
        label: "Careless Google Drive",
        isDefault: true,
        isServer: false,
      },
    ]);
    expect(parseVersion(html)).toBe("Beta 19");
    expect(parseAuthor(html)).toBe("Zattari");
  });
  it("decodes the entities the theme leaves in links", () => {
    const html = `<div class="download-container container-default"><div class="filename">dropbox</div>
      <a href="https://dl.dropbox.com/scl/fi/x/P.zip?rlkey=k&#038;st=s&#038;dl=0">Download</a></div><div id="instructions"></div>`;
    expect(parseDownloads(html)[0].url).toBe("https://dl.dropbox.com/scl/fi/x/P.zip?rlkey=k&st=s&dl=0");
  });
  it("flags server builds by label or file name, and not a track called Observer Hill", () => {
    const html = `
      <div class="download-container"><div class="filename">Server files</div><a href="https://www.mediafire.com/file/a/x.zip/file">D</a></div>
      <div class="download-container"><a href="https://www.mediafire.com/file/b/Ironman_2024_Server.pkz/file">D</a></div>
      <div class="download-container"><a href="https://www.mediafire.com/file/c/ObserverHill.zip/file">D</a></div>
      <div id="instructions"></div>`;
    expect(parseDownloads(html).map((d) => d.isServer)).toEqual([true, true, false]);
    expect(mentionsServer("observer")).toBe(false);
  });
  it("does not take OEM's information page for the recommended file", () => {
    const html = `<div class="download-container container-default"><div class="filename">Information</div>
      <a href="https://oem.mxb-mods.com/info">x</a></div>
      <div class="download-container container-recommended"><a href="https://www.mediafire.com/file/a/MX_OEM.zip/file">y</a></div><div id="instructions"></div>`;
    expect(parseDownloads(html).map((d) => d.isDefault)).toEqual([false, true]);
  });
  it("steps over the hosts' routing segments for a file name", () => {
    expect(urlFileName("https://www.mediafire.com/file/abc/track_server.zip/file")).toBe("track_server.zip");
    expect(urlFileName("https://drive.google.com/file/d/ABC123/view?usp=sharing")).toBe("ABC123");
  });
});

describe("robots.txt", () => {
  const txt = `User-agent: *\nAllow: /ads.txt\n\nUser-agent: GPTBot\nDisallow: /\n\nUser-agent: mxbsecure-mirror\nDisallow: /private/\nAllow: /private/ok$\n`;
  it("uses our own group when there is one", () => {
    const rules = robotsRules(txt);
    expect(robotsAllows(rules, "/wp-json/wp/v2/posts")).toBe(true);
    expect(robotsAllows(rules, "/private/x")).toBe(false);
    expect(robotsAllows(rules, "/private/ok")).toBe(true);
  });
  it("falls back to *, and honours a blanket disallow", () => {
    expect(robotsAllows(robotsRules("User-agent: *\nDisallow: /\n"), "/anything")).toBe(false);
    expect(robotsAllows(robotsRules("User-agent: *\nDisallow:\n"), "/anything")).toBe(true);
  });
  it("is satisfied by mxb-mods.com's real file, which blocks only AI crawlers", () => {
    const real = `User-agent: *\nAllow: /ads.txt\nAllow: /app-ads.txt\n\nUser-agent: Googlebot  # x\nAllow: /\n\nUser-agent: GPTBot  # x\nDisallow: /\n\nUser-agent: ClaudeBot\nDisallow: /\n`;
    expect(robotsAllows(robotsRules(real), "/careless-beta/")).toBe(true);
  });
});

describe("cursor and backoff", () => {
  it("steps a site-local time back a second", () => {
    expect(minusOneSecond("2026-10-07T19:53:23")).toBe("2026-10-07T19:53:22");
    expect(minusOneSecond("2026-01-01T00:00:00")).toBe("2025-12-31T23:59:59");
  });
  it("grows and caps", () => {
    const mid = () => 0.5;
    expect(backoff(1, mid)).toBe(600_000);
    expect(backoff(2, mid)).toBe(1_200_000);
    expect(backoff(20, mid)).toBe(86_400_000);
  });
});

const POST = {
  id: 191518,
  slug: "careless-beta",
  link: "https://mxb-mods.com/careless-beta/",
  modified: "2026-10-07T19:53:23",
  title: { rendered: "Careless (beta)" },
  content: { rendered: "<p>A <b>soft-soil</b> track.</p>" },
  categories: [301],
};

describe("versions", () => {
  it("a changed download list is a new version; unchanged links carry their state across", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 1000);
    const a = await e.DB.prepare("SELECT id FROM mod_assets WHERE source_ref = ?").bind(POST.id).first<{ id: number }>();
    const d = (url: string) => ({ url, host: "x", label: "L", isDefault: false, isServer: false });
    const v1 = await writeMirrorVersion(e, a!.id, "Beta 18", [d("https://a/1"), d("https://a/2")], 1000);
    await e.DB.prepare("UPDATE mod_files SET status = 'done', sha256 = 'aa' WHERE version_id = ? AND url = 'https://a/2'").bind(v1).run();

    expect(await writeMirrorVersion(e, a!.id, "Beta 18", [d("https://a/1"), d("https://a/2")], 2000)).toBe(v1);

    const v2 = await writeMirrorVersion(e, a!.id, "Beta 19", [d("https://a/2"), d("https://a/3")], 3000);
    expect(v2).not.toBe(v1);
    const { results } = await e.DB.prepare("SELECT idx, url, status, sha256 FROM mod_files WHERE version_id = ? ORDER BY idx")
      .bind(v2)
      .all();
    expect(results).toEqual([
      { idx: 0, url: "https://a/2", status: "done", sha256: "aa" },
      { idx: 1, url: "https://a/3", status: "idle", sha256: null },
    ]);
    const cur = await e.DB.prepare("SELECT current_version FROM mod_assets WHERE id = ?").bind(a!.id).first();
    expect(cur).toEqual({ current_version: v2 });
  });

  it("re-listing an unchanged post keeps its page read; a changed one re-reads it", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 1000);
    await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok'").run();
    await upsertPost(e, POST, TREE, 2000);
    expect(await e.DB.prepare("SELECT page_status, last_seen FROM mod_assets").first()).toEqual({ page_status: "ok", last_seen: 2000 });
    await upsertPost(e, { ...POST, modified: "2026-10-08T00:00:00" }, TREE, 3000);
    expect(await e.DB.prepare("SELECT page_status FROM mod_assets").first()).toEqual({ page_status: "due" });
  });

  it("never un-hides a moderated mod", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 1000);
    await e.DB.prepare("UPDATE mod_assets SET state = 'hidden'").run();
    await upsertPost(e, { ...POST, modified: "2026-10-09T00:00:00" }, TREE, 2000);
    expect(await e.DB.prepare("SELECT state FROM mod_assets").first()).toEqual({ state: "hidden" });
  });
});

describe("a whole run", () => {
  const robots = "User-agent: *\nAllow: /ads.txt\n";
  const cats = [...TREE.values()];

  function site(extra: [RegExp, () => Response][] = []) {
    return fakeFetch([
      ...extra,
      [/\/robots\.txt$/, () => new Response(robots)],
      [/\/wp-json\/wp\/v2\/categories/, () => Response.json(cats)],
      [/orderby=modified/, () => Response.json([POST])],
      [/orderby=id/, () => Response.json([{ id: POST.id }])],
      [/careless-beta\/$/, () => new Response(page("careless-beta.html"), { headers: { "content-type": "text/html" } })],
    ]);
  }

  it("discovers and indexes the page — spaced and named — but mirrors nothing unasked", async () => {
    const e = env();
    const waits: number[] = [];
    const f = site();
    await runMirror(e, { now: 10_000, fetch: f, wait: async (ms) => void waits.push(ms) });
    // The cron hands the page to the queue rather than reading it.
    expect(e.MIRROR_QUEUE.sent).toEqual([{ kind: "page", id: expect.any(Number) }]);
    expect(waits.every((w) => w === 1500)).toBe(true);
    waits.length = 0;
    expect(await drainPages(e, { now: 11_000, fetch: f, wait: async (ms) => void waits.push(ms) })).toMatchObject({ read: 1, deferred: 0 });
    const asset = await e.DB.prepare("SELECT type, author, page_status, title, description FROM mod_assets").first();
    expect(asset).toEqual({
      type: "tracks",
      author: "Zattari",
      page_status: "ok",
      title: "Careless (beta)",
      description: "A soft-soil track.",
    });
    const v = await e.DB.prepare("SELECT label FROM mod_versions").first();
    expect(v).toEqual({ label: "Beta 19" });
    expect(e.MIRROR_QUEUE.sent).toEqual([]);
    expect(await e.DB.prepare("SELECT status FROM mod_files").first()).toEqual({ status: "idle" });
    expect(waits.every((w) => w === 1000 || w === 250)).toBe(true);
    expect(f.calls.some((c) => c.includes("modified_after"))).toBe(false);

    // The next run asks only for what changed since, and re-reads nothing.
    const g = site();
    await runMirror(e, { now: 20_000, fetch: g, wait: async () => {} });
    expect(g.calls.find((c) => c.includes("orderby=modified"))).toContain("modified_after=2026-10-07T19%3A53%3A22");
    expect(e.MIRROR_QUEUE.sent).toEqual([]);
    expect(g.calls.some((c) => c.includes("careless-beta/"))).toBe(false);
  });

  /** The runtime's fetch: it refuses to run with any `this` but the global scope. */
  function workersFetch(inner: typeof fetch): typeof fetch {
    return function (this: unknown, input: RequestInfo | URL, init?: RequestInit) {
      if (this !== undefined && this !== globalThis) {
        throw new TypeError("Illegal invocation: function called with incorrect this reference");
      }
      return inner(input, init);
    } as typeof fetch;
  }

  it("calls fetch the way the runtime insists on, handed in or global", async () => {
    const handed = env();
    await runMirror(handed, { now: 10_000, fetch: workersFetch(site()), wait: async () => {} });
    await drainPages(handed, { now: 10_000, fetch: workersFetch(site()), wait: async () => {} });
    expect(await handed.DB.prepare("SELECT page_status FROM mod_assets").first()).toEqual({ page_status: "ok" });

    // No fetch handed in: the sync must reach the global one without detaching it.
    const real = globalThis.fetch;
    globalThis.fetch = workersFetch(site());
    try {
      const bare = env();
      await runMirror(bare, { now: 10_000, wait: async () => {} });
      await drainPages(bare, { now: 10_000, wait: async () => {} });
      expect(await bare.DB.prepare("SELECT page_status FROM mod_assets").first()).toEqual({ page_status: "ok" });
    } finally {
      globalThis.fetch = real;
    }
  });

  it("stops and cools down when the site refuses", async () => {
    const e = env();
    const f = site([[/orderby=modified/, () => new Response("blocked", { status: 403 })]]);
    await runMirror(e, { now: 10_000, fetch: f, wait: async () => {} });
    expect(f.calls.some((c) => c.includes("orderby=id"))).toBe(false);
    const g = site();
    await runMirror(e, { now: 20_000, fetch: g, wait: async () => {} });
    expect(g.calls).toEqual([]);
  });

  it("does nothing on the site when switched off, but still dispatches", async () => {
    const e = env({ MXB_MIRROR: "off" });
    const f = site();
    await runMirror(e, { now: 1, fetch: f, wait: async () => {} });
    expect(f.calls).toEqual([]);
  });

  it("re-dispatches a lease that ran out, and not one that hasn't", async () => {
    const e = env();
    await upsertPost(e, POST, TREE, 0);
    const a = await e.DB.prepare("SELECT id FROM mod_assets").first<{ id: number }>();
    await e.DB.prepare("UPDATE mod_assets SET page_status = 'ok'").run();
    await writeMirrorVersion(e, a!.id, null, [{ url: "https://a/1", host: "a", label: "a", isDefault: true, isServer: false }], 0);
    expect(await dispatch(e, 1000)).toBe(0);
    await e.DB.prepare("UPDATE mod_files SET status = 'pending'").run();
    expect(await dispatch(e, 1000)).toBe(1);
    expect(await dispatch(e, 2000)).toBe(0);
    expect(await dispatch(e, 1000 + 46 * 60_000)).toBe(1);
  });
});

void fixture;
