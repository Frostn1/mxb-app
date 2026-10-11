import { describe, expect, it } from "vitest";
import { MAX_SENT_PER_DAY, parseShot, parseSpec, SPEC_FORMAT } from "../src/builds";
import { webRoutes } from "../src/web";
import { sealToken, SESSION_COOKIE } from "../src/websession";
import { d1 } from "./d1sqlite";
import { fakeBucket } from "./modfakes";

const KEY = "session-secret";
const RIDER = "76561197960265730";
const ADMIN = "76561198000000043";
const STRANGER = "76561198000000099";
const SITE = "https://mxbsecure.com";
const API = "https://api.mxbsecure.com";

// A real 1×1 WebP.
const WEBP = "data:image/webp;base64,UklGRiQAAABXRUJQVlA4IBgAAAAwAQCdASoBAAEAAwA0JaQAA3AA/vuUAAA=";

function spec(over: Record<string, unknown> = {}) {
  return {
    format: SPEC_FORMAT,
    ref: "MXB-T3K9QZ",
    title: "KTM SX-F by Eyezaya",
    created: "2026-10-11T14:32:00.000Z",
    bike: { id: "eyezaya-ktm-sxf", model: "KTM SX-F", by: "Eyezaya" },
    parts: [
      {
        category: "fork",
        categoryLabel: "Fork",
        group: "Suspension",
        id: "wp-7448-aer",
        brand: "WP",
        model: "XACT PRO 7448 AER",
        by: "Frost",
        colourway: "Factory",
        page: "https://mxbsecure.com/blends/wp-7448-aer-fork",
        files: ["wp7448_aer.edf"],
      },
      {
        category: "seat",
        categoryLabel: "Seat",
        group: "Body",
        id: "onegripper-seat",
        brand: "OneGripper",
        model: "Seat",
        options: [
          { label: "Shape", value: "Hump" },
          { label: "Base colour", value: "Black #161616" },
        ],
      },
    ],
    stock: ["Shock", "Handguards"],
    ...over,
  };
}

function deployment(): Env {
  return {
    DB: d1(),
    ASSET_LOCKED: fakeBucket(),
    MXB_WEB_SESSION_KEY: KEY,
    MXB_SITE_ORIGIN: SITE,
    ADMIN_STEAM_IDS: ADMIN,
  } as unknown as Env;
}

async function cookie(steamId: string) {
  return `${SESSION_COOKIE}=${await sealToken({ t: "session", steamId, name: "Frost", exp: Date.now() + 60_000 }, KEY)}`;
}

async function call(env: Env, method: string, path: string, who: string | null, body?: unknown) {
  const headers: Record<string, string> = { Origin: SITE };
  if (who) headers.Cookie = await cookie(who);
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const r = new Request(`${API}${path}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
  return webRoutes(r, new URL(r.url), env);
}

async function json<T>(res: Response | Promise<Response>): Promise<T> {
  return (await (await res).json()) as T;
}

describe("parseSpec", () => {
  it("keeps the Build page's sheet as it is", () => {
    const r = parseSpec(spec());
    expect("spec" in r && r.spec).toEqual(spec());
  });
  it("refuses two parts in one category and a wrong format", () => {
    const fork = spec().parts[0];
    expect(parseSpec(spec({ parts: [fork, fork] }))).toHaveProperty("error");
    expect(parseSpec(spec({ format: "mxb-swap-order/1" }))).toHaveProperty("error");
    expect(parseSpec(spec({ ref: "nope" }))).toHaveProperty("error");
  });
  it("takes file names only, and links only to the site", () => {
    const fork = spec().parts[0];
    for (const f of ["../secret.blend", "C:/Users/x.blend", "Blends/x.blend", "a\\b"]) {
      expect(parseSpec(spec({ parts: [{ ...fork, files: [f] }] }))).toHaveProperty("error");
    }
    expect(parseSpec(spec({ parts: [{ ...fork, page: "https://evil.example/x" }] }))).toHaveProperty("error");
  });
  it("drops fields it doesn't know", () => {
    const r = parseSpec({ ...spec(), price: 0, admin: true });
    expect("spec" in r && Object.keys(r.spec).sort()).toEqual(["bike", "created", "format", "parts", "ref", "stock", "title"]);
  });
});

describe("parseShot", () => {
  it("takes a WebP and refuses a mislabelled one", () => {
    expect(parseShot(WEBP)).toHaveProperty("bytes");
    expect(parseShot(WEBP.replace("image/webp", "image/png"))).toHaveProperty("error");
    expect(parseShot("https://example.com/x.webp")).toHaveProperty("error");
  });
});

describe("build routes", () => {
  it("needs a sign-in", async () => {
    expect((await call(deployment(), "POST", "/v1/web/builds", null, { spec: spec() })).status).toBe(401);
  });

  it("saves a build for its owner only; the creator sees it once sent", async () => {
    const env = deployment();
    const saved = await call(env, "POST", "/v1/web/builds", RIDER, { spec: spec(), shot: WEBP });
    expect(saved.status).toBe(201);
    const { id, stage } = (await saved.json()) as { id: string; stage: string };
    expect(stage).toBe("saved");

    const mine = await json<{ builds: { id: string; steamId?: string }[] }>(call(env, "GET", "/v1/web/builds", RIDER));
    expect(mine.builds.map((b) => b.id)).toEqual([id]);
    expect(mine.builds[0].steamId).toBeUndefined();
    expect((await call(env, "GET", `/v1/web/builds/${id}`, STRANGER)).status).toBe(404);
    expect((await call(env, "GET", `/v1/web/builds/${id}/shot`, STRANGER)).status).toBe(404);
    expect((await call(env, "GET", `/v1/web/builds/${id}/shot`, RIDER)).headers.get("content-type")).toBe("image/webp");

    // Not sent: not in the queue, not even to the admin.
    expect((await json<{ builds: unknown[] }>(call(env, "GET", "/v1/web/builds?all=1", ADMIN))).builds).toEqual([]);
    expect((await call(env, "GET", `/v1/web/builds/${id}`, ADMIN)).status).toBe(404);

    expect((await call(env, "POST", `/v1/web/builds/${id}/send`, STRANGER, {})).status).toBe(404);
    expect((await call(env, "POST", `/v1/web/builds/${id}/send`, RIDER, { guids: 2, note: "Black graphics" })).status).toBe(200);
    expect((await call(env, "POST", `/v1/web/builds/${id}/send`, RIDER, {})).status).toBe(409);
    const queue = await json<{ builds: { steamId: string; guids: number; note: string; stage: string }[] }>(
      call(env, "GET", "/v1/web/builds?all=1", ADMIN),
    );
    expect(queue.builds[0]).toMatchObject({ steamId: RIDER, guids: 2, note: "Black graphics", stage: "quote" });
    // `all` from a rider is their own list, not the queue.
    expect((await json<{ builds: unknown[] }>(call(env, "GET", "/v1/web/builds?all=1", STRANGER))).builds).toEqual([]);
  });

  it("discards a saved build, never a sent one", async () => {
    const env = deployment();
    const a = await json<{ id: string }>(call(env, "POST", "/v1/web/builds", RIDER, { spec: spec(), shot: WEBP }));
    const b = await json<{ id: string }>(call(env, "POST", "/v1/web/builds", RIDER, { spec: spec(), send: true }));
    expect((await call(env, "POST", `/v1/web/builds/${a.id}/discard`, STRANGER, {})).status).toBe(404);
    expect((await call(env, "POST", `/v1/web/builds/${a.id}/discard`, RIDER, {})).status).toBe(200);
    expect((await call(env, "POST", `/v1/web/builds/${b.id}/discard`, RIDER, {})).status).toBe(409);
    expect((await json<{ builds: { id: string }[] }>(call(env, "GET", "/v1/web/builds", RIDER))).builds.map((x) => x.id)).toEqual([b.id]);
  });

  it("walks an order through the stages: quote, payment, approval, complete", async () => {
    const env = deployment();
    const { id } = await json<{ id: string }>(call(env, "POST", "/v1/web/builds", RIDER, { spec: spec(), send: true }));
    const stage = (body: unknown, who = ADMIN) => call(env, "POST", `/v1/web/builds/${id}/stage`, who, body);

    expect((await stage({ stage: "pending_payment" }, RIDER)).status).toBe(403);
    expect((await stage({ stage: "pending_payment" })).status).toBe(400); // no price yet
    expect((await stage({ stage: "paid" })).status).toBe(409); // one step at a time
    expect((await stage({ stage: "pending_payment", priceCents: 2500, note: "PayPal to frost@example" })).status).toBe(200);
    expect((await stage({ stage: "paid" })).status).toBe(200);
    expect((await stage({ stage: "approval" })).status).toBe(200);
    expect((await stage({ stage: "complete" })).status).toBe(409); // rider hasn't said yes

    const approve = (body: unknown, who = RIDER) => call(env, "POST", `/v1/web/builds/${id}/approve`, who, body);
    expect((await approve({ approve: true }, STRANGER)).status).toBe(404);
    expect((await approve({ approve: false })).status).toBe(400); // say what to change
    expect((await approve({ approve: false, note: "Seat a bit darker" })).status).toBe(200);
    expect((await stage({ stage: "complete" })).status).toBe(409);
    expect((await stage({ stage: "approval" })).status).toBe(200); // asks again
    expect((await approve({ approve: true })).status).toBe(200);
    expect((await stage({ stage: "complete" })).status).toBe(200);
    expect((await stage({ stage: "cancelled" })).status).toBe(409);

    const { build } = await json<{ build: Record<string, unknown> }>(call(env, "GET", `/v1/web/builds/${id}`, RIDER));
    expect(build).toMatchObject({ stage: "complete", priceCents: 2500, approval: "approved", adminNote: "PayPal to frost@example" });
    expect(build.steamId).toBeUndefined();
  });

  it("refuses a write from another site", async () => {
    const env = deployment();
    const r = new Request(`${API}/v1/web/builds`, {
      method: "POST",
      headers: { Origin: "https://evil.example", Cookie: await cookie(RIDER), "Content-Type": "application/json" },
      body: JSON.stringify({ spec: spec() }),
    });
    expect((await webRoutes(r, new URL(r.url), env)).status).toBe(403);
  });

  it("caps orders per day", async () => {
    const env = deployment();
    for (let i = 0; i < MAX_SENT_PER_DAY; i++) {
      expect((await call(env, "POST", "/v1/web/builds", RIDER, { spec: spec(), send: true })).status).toBe(201);
    }
    expect((await call(env, "POST", "/v1/web/builds", RIDER, { spec: spec(), send: true })).status).toBe(429);
    expect((await call(env, "POST", "/v1/web/builds", RIDER, { spec: spec() })).status).toBe(201);
  });
});
