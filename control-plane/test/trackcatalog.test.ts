import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  getTracks,
  nameAnswers,
  parseShopDump,
  resolveTrackCatalog,
  searchWords,
  trackArt,
  trackKey,
} from "../src/trackcatalog";
import { d1 } from "./d1sqlite";

/** R2, as much of it as the catalogue uses. */
function bucket() {
  const objects = new Map<string, { bytes: ArrayBuffer; type?: string }>();
  return {
    objects,
    async put(key: string, bytes: ArrayBuffer, opts?: { httpMetadata?: { contentType?: string } }) {
      objects.set(key, { bytes, type: opts?.httpMetadata?.contentType });
    },
    async get(key: string) {
      const o = objects.get(key);
      if (!o) return null;
      return { body: new Blob([o.bytes]).stream(), httpMetadata: { contentType: o.type }, httpEtag: '"x"' };
    },
    async delete(keys: string | string[]) {
      for (const k of Array.isArray(keys) ? keys : [keys]) objects.delete(k);
    },
  };
}

const ART = "https://mxb-mods.com/wp-content/uploads/farm14-768x432.jpg";

/** mxb-mods.com's REST answer for a search. */
function posts(...list: { title: string; slug: string; image?: string }[]) {
  return list.map((p, i) => ({
    id: i + 1,
    slug: p.slug,
    link: `https://mxb-mods.com/${p.slug}/`,
    title: { rendered: p.title },
    _embedded: {
      "wp:featuredmedia": [
        { source_url: "https://mxb-mods.com/full.jpg", media_details: { sizes: { medium_large: { source_url: p.image ?? ART } } } },
      ],
    },
  }));
}

let e: Env;
let r2: ReturnType<typeof bucket>;
let site: (url: URL) => Response;

beforeEach(() => {
  r2 = bucket();
  e = { DB: d1(), PAINTS: r2 } as unknown as Env;
  site = () => new Response("[]", { headers: { "content-type": "application/json" } });
  vi.stubGlobal("fetch", async (input: Request | URL | string) => {
    const url = new URL(input instanceof Request ? input.url : String(input));
    if (url.pathname.endsWith(".jpg"))
      return new Response(new Uint8Array([1, 2, 3]), { headers: { "content-type": "image/jpeg" } });
    return site(url);
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

async function ask(...ids: string[]) {
  const url = new URL("https://api.mxbsecure.com/v1/tracks");
  for (const id of ids) url.searchParams.append("id", id);
  const res = await getTracks(url, e);
  expect(res.status).toBe(200);
  return ((await res.json()) as { tracks: Record<string, any> }).tracks;
}

describe("names", () => {
  it("keys every spelling of a track the same", () => {
    expect(trackKey("Farm14")).toBe("farm14");
    expect(trackKey("farm_14")).toBe("farm14");
    expect(trackKey("KeLLz - Fox Raceway 2023")).toBe("kellzfoxraceway2023");
  });

  it("matches a spaced title, and turns away a passing mention", () => {
    expect(nameAnswers("Farm14", "Farm 14")).toBe(true);
    expect(nameAnswers("kmx_ponca_city", "KMX Ponca City")).toBe(true);
    expect(nameAnswers("forest", "Forest MX")).toBe(false);
    expect(nameAnswers("forest", "Club Forest Racing Park Winter Edition")).toBe(null);
  });

  it("searches a joined id both ways", () => {
    expect(searchWords("Farm14")).toEqual(["Farm14", "Farm 14"]);
    expect(searchWords("kmx_ponca_city")).toEqual(["kmx ponca city"]);
  });
});

describe("GET /v1/tracks", () => {
  it("queues an id it doesn't know, then answers once the cron has found it", async () => {
    expect(await ask("Farm14")).toEqual({});

    site = (url) =>
      url.searchParams.get("search") === "Farm 14"
        ? Response.json(posts({ title: "Farm 14", slug: "farm-14" }))
        : Response.json([]);
    await resolveTrackCatalog(e);

    const tracks = await ask("Farm14", "farm_14");
    expect(tracks.Farm14).toEqual({
      source: "mods",
      exact: true,
      name: "Farm 14",
      url: "https://mxb-mods.com/farm-14/",
      slug: "farm-14",
      image: "https://api.mxbsecure.com/v1/tracks/art/farm14",
      price: null,
    });
    expect(tracks.farm_14).toEqual(tracks.Farm14);

    const art = await trackArt("farm14", e);
    expect(art.status).toBe(200);
    expect(art.headers.get("content-type")).toBe("image/jpeg");
    expect(new Uint8Array(await art.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
  });

  it("tries again in an hour when the site refuses", async () => {
    await ask("walnut");
    site = () => new Response("challenge", { status: 403 });
    await resolveTrackCatalog(e);

    const row = await e.DB.prepare("SELECT source, checked_at, due_at FROM track_catalog").first<{
      source: string;
      checked_at: number;
      due_at: number;
    }>();
    expect(row?.source).toBe("");
    expect(row?.checked_at).toBe(0);
    expect(row!.due_at).toBeGreaterThan(Date.now() + 50 * 60 * 1000);
  });

  it("finds a sold track in the shop, with its price", async () => {
    e = { ...e, SHOP_CATALOG_AUTH: "bearer", SHOP_CATALOG_KEY: "k" } as Env;
    await ask("Fox Raceway 2023");
    site = (url) =>
      url.host === "mxbikes-shop.com"
        ? Response.json({
            currency: "EUR",
            categories: [{ id: 7, name: "Tracks" }],
            mods: [
              {
                id: 1,
                name: "Fox Raceway 2023",
                url: "https://mxbikes-shop.com/fox",
                image: "https://mxbikes-shop.com/fox.jpg",
                price: 9.99,
                sale_price: null,
                free: false,
                categories: [7],
              },
            ],
          })
        : Response.json([]);
    await resolveTrackCatalog(e);

    const tracks = await ask("Fox Raceway 2023");
    expect(tracks["Fox Raceway 2023"]).toMatchObject({
      source: "shop",
      exact: true,
      slug: null,
      price: { currency: "EUR", base: 9.99, sale: null, free: false },
    });
  });

  it("ignores made-up input", async () => {
    expect(await ask("", "   ", "x".repeat(65))).toEqual({});
    const n = await e.DB.prepare("SELECT COUNT(*) AS n FROM track_catalog").first<{ n: number }>();
    expect(n?.n).toBe(0);
  });
});

describe("the shop dump", () => {
  it("drops items it couldn't link to", () => {
    const items = parseShopDump({ mods: [{ name: "No link" }, { name: "Ok", url: "https://x/y", categories: ["Tracks"] }] });
    expect(items.map((i) => i.title)).toEqual(["Ok"]);
    expect(items[0].categories).toEqual(["Tracks"]);
  });
});
