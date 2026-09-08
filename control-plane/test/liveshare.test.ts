import { describe, expect, it } from "vitest";
import {
  CODE_PREFIX,
  deleteShare,
  normaliseCode,
  publishShare,
  readShare,
  updateShare,
  validManifest,
} from "../src/liveshare";
import { d1 } from "./d1sqlite";

function env(): Env {
  return { DB: d1() } as unknown as Env;
}

/** A manifest the way the app writes one: camelCase, catbox URLs, mods-relative rels. */
function manifest(over: Record<string, unknown> = {}) {
  return {
    items: [{ name: "RedBud.pkz", rel: "tracks/EU/RedBud.pkz", size: 68_000_000, isDir: false }],
    totalSize: 68_000_000,
    bundle: {
      url: "https://files.catbox.moe/abc123.zip",
      host: "catbox",
      size: 68_000_000,
      parts: ["https://files.catbox.moe/abc123.zip", "https://files.catbox.moe/def456.zip"],
      partSizes: [34_000_000, 34_000_000],
    },
    ...over,
  };
}

function post(body: unknown): Request {
  return new Request("https://cp.invalid/v1/share", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

function put(body: unknown, key?: string): Request {
  return new Request("https://cp.invalid/v1/share/X", {
    method: "PUT",
    headers: key ? { "x-update-key": key } : {},
    body: JSON.stringify(body),
  });
}

function get(etag?: string): Request {
  return new Request("https://cp.invalid/v1/share/X", {
    headers: etag ? { "if-none-match": etag } : {},
  });
}

/** Publish once, and hand back what the app would have stored. */
async function published(e: Env, name = "RedBud 2026") {
  const resp = await publishShare(post({ name, manifest: manifest() }), e);
  expect(resp.status).toBe(201);
  return (await resp.json()) as { code: string; updateKey: string; version: number };
}

describe("codes", () => {
  it("reads a code however it was typed", () => {
    const printed = "MXBL1-K7QP4M2X";
    for (const typed of [printed, printed.toLowerCase(), "k7qp4m2x", "MXBL1-K7QP 4M2X", "K7QP-4M2X"]) {
      expect(normaliseCode(typed), typed).toBe("K7QP4M2X");
    }
  });

  /** The reason the alphabet drops these: someone reads a code aloud and the listener has
   *  to guess whether they said "oh" or "zero". Both must resolve to the same share. */
  it("folds the letters Crockford treats as digits", () => {
    expect(normaliseCode("O1IL2345")).toBe("01112345");
  });

  it("refuses anything that isn't a code", () => {
    expect(normaliseCode("K7QP4M2")).toBeNull();
    expect(normaliseCode("K7QP4M2XY")).toBeNull();
    expect(normaliseCode("K7QP4M2U"), "U is not in the alphabet").toBeNull();
    expect(normaliseCode(42)).toBeNull();
  });
});

describe("manifest validation", () => {
  it("takes what the app writes", () => {
    expect(validManifest(manifest())).toBe(true);
  });

  /**
   * The one that matters. Every rel is joined onto the receiver's mods root at install
   * time, so a manifest carrying `..` would write outside it — and this endpoint takes an
   * unauthenticated POST, so nothing upstream can be relied on to have checked.
   */
  it("refuses a rel that climbs out of the mods folder", () => {
    for (const rel of ["../../secrets.txt", "/etc/passwd", "C:/Windows/system32", "a/../../b"]) {
      expect(validManifest(manifest({ items: [{ name: "x", rel, size: 1, isDir: false }] })), rel)
        .toBe(false);
    }
  });

  /** Fail closed: this table must never be usable to hand out a link to anything else. */
  it("refuses a bundle pointing anywhere but the upload hosts", () => {
    const bad = (url: string) => manifest({ bundle: { url, host: "catbox", size: 1 } });
    expect(validManifest(bad("https://evil.invalid/x.zip"))).toBe(false);
    expect(validManifest(bad("http://files.catbox.moe/x.zip")), "http, not https").toBe(false);
    expect(validManifest(bad("https://files.catbox.moe.evil.invalid/x.zip"))).toBe(false);
    expect(validManifest(bad("https://files.catbox.moe/x.zip"))).toBe(true);
  });

  it("refuses a part pointing somewhere else, even when the first URL is fine", () => {
    const m = manifest();
    m.bundle.parts = ["https://files.catbox.moe/a.zip", "https://evil.invalid/b.zip"];
    expect(validManifest(m)).toBe(false);
  });

  it("refuses an empty share", () => {
    expect(validManifest(manifest({ items: [] }))).toBe(false);
  });
});

describe("publishing", () => {
  it("mints a code and an update key", async () => {
    const e = env();
    const out = await published(e);
    expect(out.code.startsWith(CODE_PREFIX)).toBe(true);
    expect(normaliseCode(out.code)).not.toBeNull();
    expect(out.updateKey.length).toBeGreaterThan(20);
    expect(out.version).toBe(1);
  });

  it("refuses a bad manifest before it reaches the table", async () => {
    const e = env();
    const bad = manifest({ items: [{ name: "x", rel: "../x", size: 1, isDir: false }] });
    const resp = await publishShare(post({ name: "n", manifest: bad }), e);
    expect(resp.status).toBe(400);
    const row = await e.DB.prepare(`SELECT count(*) AS n FROM live_shares`).first<{ n: number }>();
    expect(row?.n).toBe(0);
  });

  it("refuses a name with control characters in it", async () => {
    const resp = await publishShare(post({ name: "Red\u0000Bud", manifest: manifest() }), env());
    expect(resp.status).toBe(400);
  });
});

describe("updating", () => {
  /** The whole feature: the code does not move, the version does. */
  it("keeps the code and bumps the version", async () => {
    const e = env();
    const first = await published(e);

    const next = manifest({ totalSize: 71_000_000 });
    const resp = await updateShare(put({ name: "RedBud 2026", manifest: next }, first.updateKey), first.code, e);
    expect(resp.status).toBe(200);
    const out = (await resp.json()) as { code: string; version: number; size: number };
    expect(out.code).toBe(first.code);
    expect(out.version).toBe(2);
    expect(out.size).toBe(71_000_000);
  });

  it("refuses a stranger holding the public code", async () => {
    const e = env();
    const first = await published(e);

    const resp = await updateShare(put({ name: "not yours", manifest: manifest() }, "guessed"), first.code, e);
    expect(resp.status).toBe(403);

    // And the share is untouched — a refused write must not have half-happened.
    const still = (await (await readShare(get(), first.code, e)).json()) as {
      name: string;
      version: number;
    };
    expect(still.name).toBe("RedBud 2026");
    expect(still.version).toBe(1);
  });

  it("refuses an update with no key at all", async () => {
    const e = env();
    const first = await published(e);
    const resp = await updateShare(put({ name: "x", manifest: manifest() }), first.code, e);
    expect(resp.status).toBe(401);
  });

  it("will not update a code that doesn't exist", async () => {
    const resp = await updateShare(put({ name: "x", manifest: manifest() }, "key"), "K7QP4M2X", env());
    expect(resp.status).toBe(403);
  });
});

describe("reading", () => {
  it("hands back the manifest and tags it with the version", async () => {
    const e = env();
    const first = await published(e);
    const resp = await readShare(get(), first.code, e);
    expect(resp.status).toBe(200);
    expect(resp.headers.get("etag")).toBe('"1"');
    const out = (await resp.json()) as { code: string; manifest: { items: { rel: string }[] } };
    expect(out.code).toBe(first.code);
    expect(out.manifest.items[0].rel).toBe("tracks/EU/RedBud.pkz");
  });

  /** What nearly every check costs: nothing but the round trip. */
  it("answers 304 when the subscriber already has this version", async () => {
    const e = env();
    const first = await published(e);
    const resp = await readShare(get('"1"'), first.code, e);
    expect(resp.status).toBe(304);
    expect(await resp.text()).toBe("");
  });

  it("answers with the body again once the version moves", async () => {
    const e = env();
    const first = await published(e);
    await updateShare(put({ name: "RedBud 2026", manifest: manifest() }, first.updateKey), first.code, e);

    const resp = await readShare(get('"1"'), first.code, e);
    expect(resp.status).toBe(200);
    expect(resp.headers.get("etag")).toBe('"2"');
  });

  /** A proxy may weaken the tag it echoes back, and a conditional request may carry a list. */
  it("takes a weak tag and a list", async () => {
    const e = env();
    const first = await published(e);
    expect((await readShare(get('W/"1"'), first.code, e)).status).toBe(304);
    expect((await readShare(get('"0", "1"'), first.code, e)).status).toBe(304);
  });

  it("never hands out the update key", async () => {
    const e = env();
    const first = await published(e);
    const text = await (await readShare(get(), first.code, e)).text();
    expect(text).not.toContain(first.updateKey);
    expect(text).not.toContain("updateHash");
  });

  it("404s an unknown code", async () => {
    expect((await readShare(get(), "K7QP4M2X", env())).status).toBe(404);
    expect((await readShare(get(), "nonsense", env())).status).toBe(404);
  });
});

describe("unpublishing", () => {
  it("takes the code out of service for its owner", async () => {
    const e = env();
    const first = await published(e);
    const del = new Request("https://cp.invalid/x", {
      method: "DELETE",
      headers: { "x-update-key": first.updateKey },
    });
    expect((await deleteShare(del, first.code, e)).status).toBe(200);
    expect((await readShare(get(), first.code, e)).status).toBe(404);
  });

  it("refuses a stranger", async () => {
    const e = env();
    const first = await published(e);
    const del = new Request("https://cp.invalid/x", {
      method: "DELETE",
      headers: { "x-update-key": "guessed" },
    });
    expect((await deleteShare(del, first.code, e)).status).toBe(403);
    expect((await readShare(get(), first.code, e)).status).toBe(200);
  });
});
