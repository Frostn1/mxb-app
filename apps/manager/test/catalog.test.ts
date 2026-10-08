import { afterEach, describe, expect, test } from "bun:test";
import {
  MIRROR_HOST,
  catalogDownload,
  catalogInstallType,
  pickCatalogFile,
  preferMirror,
  sameLink,
  type CatalogFile,
} from "@frost/shared/api/catalog";

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

/** Answer each request with the next body in `bodies`, recording what was asked. */
function stubFetch(bodies: [number, unknown][]) {
  const asked: { url: string; method: string }[] = [];
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    asked.push({ url: String(input), method: init?.method ?? "GET" });
    const [status, body] = bodies.shift() ?? [500, { error: "no more" }];
    return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
  }) as typeof fetch;
  return asked;
}

const MOD = { id: "4da58750-e032-431b-977d-d086e5fffd31", version_seq: 1 };
const file = (over: Partial<CatalogFile> = {}): CatalogFile => ({
  idx: 0,
  part: 0,
  path: null,
  host: "drive.google.com",
  server: false,
  recommended: true,
  state: "original",
  filename: null,
  size: null,
  locked: false,
  download: "https://api.mxbsecure.com/v1/assets/x/download/0?version=1",
  source: "https://drive.google.com/file/d/abc/view",
  ...over,
});

describe("catalogInstallType", () => {
  test("puts each catalog type in a folder the app knows", () => {
    expect(catalogInstallType("tracks")?.modType.installSubpath).toBe("mods/tracks");
    expect(catalogInstallType("bikes")?.modType.installSubpath).toBe("mods/bikes");
    // Liveries go to a bike's paints: the Liveries category is what routes them.
    expect(catalogInstallType("liveries")).toMatchObject({ categoryId: 37 });
    expect(catalogInstallType("kits")?.modType.id).toBe("rider");
    expect(catalogInstallType("paints")?.modType.installSubpath).toBe("auto");
    expect(catalogInstallType("nope")).toBeNull();
  });
});

describe("pickCatalogFile", () => {
  test("takes the recommended playable file, never a server build when there is another", () => {
    const server = file({ idx: 0, server: true, recommended: true });
    const plain = file({ idx: 1, recommended: false });
    const rec = file({ idx: 2, recommended: true });
    expect(pickCatalogFile([server, plain, rec])).toBe(rec);
    expect(pickCatalogFile([server, plain])).toBe(plain);
    expect(pickCatalogFile([server])).toBe(server);
    // A folder's parts are installed through the share itself.
    expect(pickCatalogFile([file({ part: 1 })])).toBeNull();
  });
});

test("sameLink ignores a trailing slash and the scheme", () => {
  expect(sameLink("https://www.mediafire.com/file/x/", "http://www.mediafire.com/file/x")).toBe(true);
  expect(sameLink("https://a/x", "https://a/y")).toBe(false);
  expect(sameLink(null, "https://a/x")).toBe(false);
});

describe("catalogDownload", () => {
  test("a stored file comes from us without asking", async () => {
    const asked = stubFetch([]);
    expect(await catalogDownload(MOD, file({ state: "stored" }), { wait: true })).toEqual({
      url: "https://api.mxbsecure.com/v1/assets/x/download/0?version=1",
      host: MIRROR_HOST,
    });
    expect(asked).toEqual([]);
  });

  test("queues the copy and waits for it", async () => {
    const asked = stubFetch([
      [200, { state: "mirroring" }],
      [200, { state: "mirroring" }],
      [200, { state: "stored", download: "https://api.mxbsecure.com/v1/assets/x/download/0?version=1" }],
    ]);
    const got = await catalogDownload(MOD, file(), { wait: true, sleep: async () => {} });
    expect(got).toEqual({ url: "https://api.mxbsecure.com/v1/assets/x/download/0?version=1", host: MIRROR_HOST });
    expect(asked).toHaveLength(3);
    expect(asked[0]).toEqual({ url: `https://api.mxbsecure.com/v1/assets/${MOD.id}/prepare/0?version=1`, method: "POST" });
  });

  test("falls back to the original when the mirror can't take it, or time runs out", async () => {
    stubFetch([[200, { state: "original", source: "https://drive.google.com/file/d/abc/view" }]]);
    expect(await catalogDownload(MOD, file(), { wait: true })).toEqual({
      url: "https://drive.google.com/file/d/abc/view",
      host: "drive.google.com",
    });

    let clock = 0;
    stubFetch(Array.from({ length: 200 }, () => [200, { state: "mirroring" }] as [number, unknown]));
    const late = await catalogDownload(MOD, file(), {
      wait: true,
      now: () => clock,
      sleep: async (ms) => {
        clock += ms;
      },
    });
    expect(late?.host).toBe("drive.google.com");

    stubFetch([[503, { error: "down" }]]);
    expect((await catalogDownload(MOD, file(), { wait: true }))?.host).toBe("drive.google.com");
  });
});

describe("preferMirror", () => {
  const mirror = { url: "https://drive.google.com/file/d/abc/view", host: "Google Drive", label: "x" };

  test("swaps in our stored copy of the same file", async () => {
    const asked = stubFetch([[200, { ...MOD, files: [file({ state: "stored" })] }]]);
    expect(await preferMirror("mxb", "hillsford-mx-park", mirror)).toEqual({
      url: "https://api.mxbsecure.com/v1/assets/x/download/0?version=1",
      host: MIRROR_HOST,
    });
    expect(asked[0].url).toBe("https://api.mxbsecure.com/v1/assets/mirror/hillsford-mx-park");
  });

  test("keeps the original when we don't hold it, and asks for a copy", async () => {
    const asked = stubFetch([[200, { ...MOD, files: [file()] }], [200, { state: "mirroring" }]]);
    expect(await preferMirror("mxb", "hillsford-mx-park", mirror)).toBe(mirror);
    await new Promise((r) => setTimeout(r, 0));
    expect(asked[1].method).toBe("POST");
  });

  test("never gets in the way", async () => {
    stubFetch([[404, { error: "no such mod" }]]);
    expect(await preferMirror("mxb", "unknown", mirror)).toBe(mirror);
    const asked = stubFetch([]);
    expect(await preferMirror("gpb", "x", mirror)).toBe(mirror);
    expect(asked).toEqual([]);
    globalThis.fetch = (async () => {
      throw new Error("offline");
    }) as unknown as typeof fetch;
    expect(await preferMirror("mxb", "x", mirror)).toBe(mirror);
  });
});
