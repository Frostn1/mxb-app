import { describe, expect, it, vi } from "vitest";
import {
  claimInvite,
  deploy,
  enrollBox,
  hostingTick,
  mintInvite,
  msmClaim,
  msmLink,
  myHosting,
  nativeConfig,
  operatorBox,
  operatorView,
  rememberHostName,
  ownerDelete,
  updateSettings,
  addTrack,
  type Deps,
} from "../src/hosting";
import { hostedRoutes } from "../src/hostingroutes";
import { REGIONS } from "../src/hostregions";
import type { OvhClient } from "../src/ovh";
import { d1 } from "./d1sqlite";

const BOSS = "76561190000000001";
const RIDER = "76561190000000002";
const RIDER2 = "76561190000000003";
const ENROLL_KEY = "k".repeat(40);
const DAY = 24 * 60 * 60 * 1000;

function env(vars: Record<string, string> = {}): Env {
  return {
    DB: d1(),
    ADMIN_STEAM_IDS: BOSS,
    MXB_HOST_SPEND_CAP_USD: "30",
    MXB_HOST_BOX_PRICE_USD: "5.85",
    MXB_HOST_MAX_BOXES: "4",
    MXB_HOST_LEGACY_MAX_BOXES: "1",
    MXB_HOST_SLOTS_NATIVE: "4",
    MXB_HOST_SLOTS_LEGACY: "2",
    MXB_HOST_SSH_PUBLIC_KEY: "ssh-ed25519 AAAA test",
    MXB_GH_DISPATCH_TOKEN: "gh-token",
    MXB_BOX_ENROLL_KEY: ENROLL_KEY,
    ...vars,
  } as unknown as Env;
}

/** A fake OVH: orders get increasing ids, and an order is delivered once `deliver` is called. */
function fakeOvh() {
  let next = 100;
  const delivered = new Map<number, string>();
  const ovh = {
    orderVps: vi.fn(async () => ({ orderId: ++next, price: 8.1, currency: "USD" })),
    deliveredService: vi.fn(async (id: number) => delivered.get(id) ?? null),
    ipv4: vi.fn(async (svc: string) => `51.81.10.${svc.length}`),
    renewsAt: vi.fn(async () => Date.parse("2026-11-07")),
    rebuild: vi.fn(async () => undefined),
    busy: vi.fn(async () => false),
    vps: vi.fn(async () => ({ state: "running" })),
  };
  return { ovh, deliver: (orderId: number, svc: string) => delivered.set(orderId, svc) };
}

/** Slots and GitHub. Records every call; slots answer OK. */
function fakeFetch() {
  const calls: { url: string; init?: RequestInit }[] = [];
  const f = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });
    if (url.startsWith("https://api.github.com/")) return new Response(null, { status: 204 });
    if (url.endsWith("/v1/riders")) return new Response(JSON.stringify({ riders: [] }));
    return new Response(JSON.stringify({ ok: true }));
  });
  return { fetch: f as unknown as typeof fetch, calls };
}

function deps(ovh: ReturnType<typeof fakeOvh>["ovh"] | null, f: ReturnType<typeof fakeFetch>, clock: { t: number }): Deps {
  return { fetch: f.fetch, now: () => clock.t, ovh: ovh as unknown as OvhClient };
}

async function invited(e: Env, d: Deps, steamId: string, quota = 1): Promise<void> {
  const minted = await mintInvite(e, d, BOSS, { steamId, quota });
  expect(minted.status).toBe(201);
  const claimed = await claimInvite(e, d, steamId, { code: (minted.body as { code: string }).code });
  expect(claimed.status).toBe(200);
}

async function serverId(r: { body: unknown }): Promise<string> {
  return (r.body as { server: { id: string } }).server.id;
}

describe("invites", () => {
  it("are single use, can be bound to one Steam account, and expire", async () => {
    const e = env();
    const clock = { t: Date.parse("2026-10-07") };
    const d = deps(null, fakeFetch(), clock);
    const bound = await mintInvite(e, d, BOSS, { steamId: RIDER });
    const code = (bound.body as { code: string; link: string }).code;
    expect((bound.body as { link: string }).link).toBe(`https://servers.mxbsecure.com/invite/${code}`);
    expect((await claimInvite(e, d, RIDER2, { code })).status).toBe(403);
    expect((await claimInvite(e, d, RIDER, { code })).status).toBe(200);
    expect((await claimInvite(e, d, RIDER2, { code })).status).toBe(409);

    const open = (await mintInvite(e, d, BOSS, { expiresDays: 1 })).body as { code: string };
    clock.t += 2 * DAY;
    expect((await claimInvite(e, d, RIDER2, { code: open.code })).status).toBe(404);
    expect(((await myHosting(e, RIDER2, false)).body as { invited: boolean }).invited).toBe(false);
  });

  it("gates deploy", async () => {
    const e = env();
    const d = deps(fakeOvh().ovh, fakeFetch(), { t: 1 });
    const r = await deploy(e, d, RIDER, { name: "x", type: "mxbserver", region: "us-east" });
    expect(r.status).toBe(403);
  });
});

describe("operator view names", () => {
  it("names owners by Steam name, never by SteamID, and returns claim and poll times", async () => {
    const e = env();
    const d = deps(fakeOvh().ovh, fakeFetch(), { t: Date.now() });
    await invited(e, d, RIDER);
    await rememberHostName(e, RIDER, "Rider One");
    await e.DB.prepare(
      "INSERT INTO host_servers (id, steam_id, name, type, region, state, max_riders, last_active_at, created_at, polled_at) VALUES ('s1', ?, 'Club', 'mxbserver', 'x', 'ready', 8, 1, 1, 777)",
    ).bind(RIDER).run();
    const view = (await operatorView(e)).body as {
      users: { name: string }[];
      servers: { ownerName: string; polledAt: number }[];
      invites: { claimedByName: string; claimedAt: number }[];
    };
    expect(view.users[0].name).toBe("Rider One");
    expect(view.servers[0]).toMatchObject({ ownerName: "Rider One", polledAt: 777 });
    expect(view.invites[0].claimedByName).toBe("Rider One");
    expect(view.invites[0].claimedAt).toBeGreaterThan(0);
    expect(JSON.stringify(view)).not.toContain(RIDER);
  });
});

describe("deploy, placement and the spend cap", () => {
  it("orders nothing with no cap, and tells the user plainly", async () => {
    const e = env({ MXB_HOST_SPEND_CAP_USD: "0" });
    const { ovh } = fakeOvh();
    const d = deps(ovh, fakeFetch(), { t: 1 });
    await invited(e, d, RIDER);
    const r = await deploy(e, d, RIDER, { name: "Track day", type: "mxbserver", region: "eu-west" });
    expect(r).toEqual({ status: 409, body: { code: "no_capacity", error: "No capacity in EU West right now." } });
    expect(ovh.orderVps).not.toHaveBeenCalled();
    const view = (await operatorView(e)).body as { alerts: { kind: string; message: string }[] };
    expect(view.alerts[0].kind).toBe("capacity");
    expect(view.alerts[0].message).toContain("spend cap");
  });

  it("orders a box in the user's region from the mapped OVH plan and datacenter", async () => {
    for (const region of REGIONS) {
      const e = env();
      const { ovh } = fakeOvh();
      const d = deps(ovh, fakeFetch(), { t: 1 });
      await invited(e, d, RIDER);
      const r = await deploy(e, d, RIDER, { name: "Mine", type: "mxbserver", region: region.id });
      expect(r.status).toBe(201);
      expect(ovh.orderVps).toHaveBeenCalledWith({
        subsidiary: "US",
        planCode: region.planCode,
        datacenter: region.datacenter,
        os: "Ubuntu 24.04",
        addons: [region.osAddon, region.storageAddon, region.backupAddon],
      });
      const server = (r.body as { server: { state: string; progress: { step: number; note: string } } }).server;
      expect(server.state).toBe("provisioning");
      expect(server.progress.step).toBe(1);
      expect(server.progress.note).toBe("A new server can take from a few minutes up to a day.");
    }
  });

  it("packs a second deploy onto the box already on its way, and stops at the cap", async () => {
    // Cap 12 = two boxes at 5.85.
    const e = env({ MXB_HOST_SPEND_CAP_USD: "12" });
    const { ovh } = fakeOvh();
    const d = deps(ovh, fakeFetch(), { t: 1 });
    const riders = Array.from({ length: 10 }, (_, i) => `7656119000000010${i}`);
    for (const r of riders) await invited(e, d, r);
    for (let i = 0; i < 4; i++) {
      expect((await deploy(e, d, riders[i], { name: `s${i}`, type: "mxbserver", region: "us-east" })).status).toBe(201);
    }
    expect(ovh.orderVps).toHaveBeenCalledTimes(1); // four slots on one box
    expect((await deploy(e, d, riders[4], { name: "s4", type: "mxbserver", region: "us-west" })).status).toBe(201);
    expect(ovh.orderVps).toHaveBeenCalledTimes(2);
    const refused = await deploy(e, d, riders[5], { name: "s5", type: "mxbserver", region: "oceania" });
    expect(refused).toMatchObject({ status: 409, body: { error: "No capacity in Oceania right now." } });
    expect(ovh.orderVps).toHaveBeenCalledTimes(2);
    const spend = ((await operatorView(e)).body as { spend: { committedUsd: number; boxes: number } }).spend;
    expect(spend).toMatchObject({ committedUsd: 11.7, boxes: 2 });
  });

  it("keeps the box limit as a second guard, and Legacy to one trial box", async () => {
    const e = env({ MXB_HOST_SPEND_CAP_USD: "1000", MXB_HOST_MAX_BOXES: "1" });
    const { ovh } = fakeOvh();
    const d = deps(ovh, fakeFetch(), { t: 1 });
    await invited(e, d, RIDER);
    await invited(e, d, RIDER2);
    expect((await deploy(e, d, RIDER, { name: "a", type: "mxbserver", region: "us-east" })).status).toBe(201);
    expect((await deploy(e, d, RIDER2, { name: "b", type: "mxbserver", region: "eu-east" })).status).toBe(409);

    const e2 = env({ MXB_HOST_SPEND_CAP_USD: "1000" });
    const d2 = deps(fakeOvh().ovh, fakeFetch(), { t: 1 });
    for (const r of [RIDER, RIDER2, BOSS]) await invited(e2, d2, r);
    expect((await deploy(e2, d2, RIDER, { name: "a", type: "legacy", region: "us-east" })).status).toBe(201);
    expect((await deploy(e2, d2, RIDER2, { name: "b", type: "legacy", region: "us-east" })).status).toBe(201); // 2 slots
    const third = await deploy(e2, d2, BOSS, { name: "c", type: "legacy", region: "eu-west" });
    expect(third.status).toBe(409);
    const alerts = ((await operatorView(e2)).body as { alerts: { message: string }[] }).alerts;
    expect(alerts[0].message).toContain("Legacy box limit");
  });

  it("holds each account to its quota", async () => {
    const e = env();
    const d = deps(fakeOvh().ovh, fakeFetch(), { t: 1 });
    await invited(e, d, RIDER);
    expect((await deploy(e, d, RIDER, { name: "a", type: "mxbserver", region: "us-east" })).status).toBe(201);
    const second = await deploy(e, d, RIDER, { name: "b", type: "mxbserver", region: "us-east" });
    expect(second).toMatchObject({ status: 403, body: { code: "quota" } });
  });
});

describe("a box from order to ready", () => {
  it("delivers, rebuilds, dispatches the installer, enrolls and seats the waiting server", async () => {
    const e = env();
    const { ovh, deliver } = fakeOvh();
    const f = fakeFetch();
    const clock = { t: Date.parse("2026-10-07T10:00:00Z") };
    const d = deps(ovh, f, clock);
    await invited(e, d, RIDER);
    expect((await addTrack(e, d, { pool: "native", name: "Club MX", url: "https://example.com/club.pkz", sha256: "a".repeat(64) })).status).toBe(201);
    const id = await serverId(await deploy(e, d, RIDER, { name: "Sean's", type: "mxbserver", region: "us-east" }));

    await hostingTick(e, d); // not delivered yet
    expect(ovh.rebuild).not.toHaveBeenCalled();
    deliver(101, "vps-abc.vps.ovh.us");
    await hostingTick(e, d);
    expect(ovh.rebuild).toHaveBeenCalledWith("vps-abc.vps.ovh.us", "Ubuntu 24.04", "ssh-ed25519 AAAA test");

    clock.t += 3 * 60 * 1000;
    await hostingTick(e, d);
    const dispatch = f.calls.find((c) => c.url.includes("api.github.com"))!;
    expect(dispatch.url).toBe("https://api.github.com/repos/Frostn1/mxbserver-releases/actions/workflows/box-install.yml/dispatches");
    const inputs = JSON.parse(String(dispatch.init!.body)).inputs;
    expect(inputs).toMatchObject({ ip: "51.81.10.18", pool: "native", slots: "4" });
    const view = (await myHosting(e, RIDER, false)).body as { servers: { state: string; progress: { step: number } }[] };
    expect(view.servers[0]).toMatchObject({ state: "installing", progress: { step: 2 } });

    const boxId = inputs.box_id as string;
    const tokens = [1, 2, 3, 4].map((i) => ({ index: i, gamePort: 54209 + i, token: `cp-s${i}.${"f".repeat(64)}` }));
    expect((await enrollBox(e, d, boxId, { slots: tokens.slice(1) })).status).toBe(400);
    expect((await enrollBox(e, d, boxId, { slots: tokens.map((t) => ({ ...t, gamePort: 1 })) })).status).toBe(400);
    expect((await enrollBox(e, d, boxId, { slots: tokens })).status).toBe(200);

    const ready = (await myHosting(e, RIDER, false)).body as { servers: Record<string, unknown>[] };
    expect(ready.servers[0]).toMatchObject({ id, state: "ready", address: "51.81.10.18:54210", progress: { step: 3 } });
    // The settings went to the slot through Caddy, with the slot's own token, and no token
    // ever comes back out.
    const write = f.calls.find((c) => c.url === "https://51-81-10-18.sslip.io/s1/v1/config/write")!;
    expect((write.init!.headers as Record<string, string>).Authorization).toBe(`Bearer ${tokens[0].token}`);
    expect(JSON.parse(String(write.init!.body)).content).toContain('package = "/etc/mxbserver/tracks/n-club-mx.pkz"');
    expect(JSON.stringify(ready)).not.toContain("ffff");
    expect(JSON.stringify((await operatorView(e)).body)).not.toContain("ffff");
  });

  it("fails the waiting servers and alerts when the install fails", async () => {
    const e = env();
    const { ovh, deliver } = fakeOvh();
    const clock = { t: 1_000_000 };
    const d = deps(ovh, fakeFetch(), clock);
    await invited(e, d, RIDER);
    await deploy(e, d, RIDER, { name: "x", type: "legacy", region: "oceania" });
    deliver(101, "vps-x.vps.ovh.us");
    await hostingTick(e, d);
    const boxId = ((await operatorView(e)).body as { boxes: { id: string }[] }).boxes[0].id;
    const res = await hostedRoutes(
      new Request(`https://api/v1/hosting/boxes/${boxId}/stage`, {
        method: "POST",
        headers: { Authorization: `Bearer ${ENROLL_KEY}` },
        body: JSON.stringify({ stage: "failed", ok: false, log: "wine: not found" }),
      }),
      new URL(`https://api/v1/hosting/boxes/${boxId}/stage`),
      e,
      d,
    );
    expect(res.status).toBe(200);
    const view = (await myHosting(e, RIDER, false)).body as { servers: { state: string; error: string }[] };
    expect(view.servers[0].state).toBe("failed");
    const op = (await operatorView(e)).body as { alerts: { kind: string }[]; spend: { boxes: number } };
    expect(op.alerts.map((a) => a.kind)).toContain("install_failed");
    expect(op.spend.boxes).toBe(1); // delivered, so still billed until cancelled at OVH
  });

  it("refuses the runner routes without the enroll key", async () => {
    const e = env();
    const res = await hostedRoutes(new Request("https://api/v1/hosting/tracks?pool=native"), new URL("https://api/v1/hosting/tracks?pool=native"), e);
    expect(res.status).toBe(403);
  });
});

async function readyServer(vars: Record<string, string> = {}) {
  const e = env(vars);
  const { ovh, deliver } = fakeOvh();
  const f = fakeFetch();
  const clock = { t: Date.parse("2026-10-07T10:00:00Z") };
  const d = deps(ovh, f, clock);
  await invited(e, d, RIDER);
  await addTrack(e, d, { pool: "native", name: "Club MX", url: "https://example.com/c.pkz", sha256: "a".repeat(64) });
  await addTrack(e, d, { pool: "native", name: "Sand", url: "https://example.com/s.pkz", sha256: "b".repeat(64) });
  const id = await serverId(await deploy(e, d, RIDER, { name: "Mine", type: "mxbserver", region: "us-east" }));
  deliver(101, "vps-abc.vps.ovh.us");
  await hostingTick(e, d);
  clock.t += 3 * 60 * 1000;
  await hostingTick(e, d);
  const boxId = ((await operatorView(e)).body as { boxes: { id: string }[] }).boxes[0].id;
  await enrollBox(e, d, boxId, {
    slots: [1, 2, 3, 4].map((i) => ({ index: i, gamePort: 54209 + i, token: `cp-s${i}.${"f".repeat(64)}` })),
  });
  return { e, d, f, clock, id, boxId, ovh };
}

describe("what an owner may change", () => {
  it("takes a track from the box set, a bike set and a rider cap, and nothing else", async () => {
    const { e, d, f, id } = await readyServer();
    expect((await updateSettings(e, d, RIDER, id, { track: "nope" })).status).toBe(400);
    expect((await updateSettings(e, d, RIDER, id, { maxRiders: 31 })).status).toBe(400);
    expect((await updateSettings(e, d, RIDER, id, { bikeSet: "shop-85" })).status).toBe(400);
    expect((await updateSettings(e, d, RIDER2, id, { maxRiders: 10 })).status).toBe(404);
    const ok = await updateSettings(e, d, RIDER, id, { track: "n-sand", bikeSet: "oem-mx1", maxRiders: 12 });
    expect(ok.status).toBe(200);
    const last = f.calls.filter((c) => c.url.endsWith("/v1/config/write")).at(-1)!;
    const content = JSON.parse(String(last.init!.body)).content as string;
    expect(content).toContain("max_clients = 12");
    expect(content).toContain('listen = "0.0.0.0:54210"');
    expect(content).toContain("/etc/mxbserver/bike-sets/oem-mx1.toml");
    expect(content).toContain('listen = "127.0.0.1:9810"');
  });

  it("renders a name that can't break out of its TOML string", () => {
    const toml = nativeConfig({ name: 'a"b', index: 2, maxRiders: 99, track: "t", bikeSet: null });
    expect(toml).toContain('name = "a\\"b"');
    expect(toml).toContain("max_clients = 30");
    expect(toml).toContain('listen = "0.0.0.0:54211"');
  });
});

describe("MSM", () => {
  it("swaps a one-time claim for a bearer that drives that server only", async () => {
    const { e, d, id } = await readyServer();
    const link = (await msmLink(e, d, RIDER, id)).body as { url: string; code: string };
    expect(link.url).toMatch(/^mxbservers:\/\/hosted\/claim\?code=[A-Za-z0-9_-]{16,128}$/);
    const claim = new URL(link.url).searchParams.get("code")!;
    expect(claim).toBe(link.code);
    const first = await msmClaim(e, d, { claim });
    expect(first.status).toBe(200);
    expect((await msmClaim(e, d, { claim })).status).toBe(404);
    const token = (first.body as { token: string }).token;

    const get = (path: string, bearer: string, init: RequestInit = {}) =>
      hostedRoutes(new Request(`https://api${path}`, { ...init, headers: { Authorization: `Bearer ${bearer}` } }), new URL(`https://api${path}`), e, d);
    const status = await get(`/v1/hosted/servers/${id}`, token);
    expect(status.status).toBe(200);
    expect(((await status.json()) as { server: { address: string } }).server.address).toBe("51.81.10.18:54210");
    expect((await get(`/v1/hosted/servers/${id}`, "wrong")).status).toBe(401);
    expect((await get(`/v1/hosted/servers/${crypto.randomUUID()}`, token)).status).toBe(401);
    const set = await get(`/v1/hosted/servers/${id}/settings`, token, { method: "PUT", body: JSON.stringify({ maxRiders: 8 }) });
    expect(set.status).toBe(200);

    expect((await ownerDelete(e, d, RIDER, id)).status).toBe(200);
    expect((await get(`/v1/hosted/servers/${id}`, token)).status).toBe(401);
  });
});

describe("scale-down that fits monthly billing", () => {
  it("frees an idle slot after 7 days and flags an empty box near its renewal, never cancelling", async () => {
    const { e, d, clock, boxId, id } = await readyServer();
    clock.t += 8 * DAY;
    await hostingTick(e, d);
    let view = (await operatorView(e)).body as { boxes: { state: string }[]; servers: unknown[] };
    expect(view.servers).toHaveLength(0); // reclaimed
    expect(view.boxes[0].state).toBe("ready"); // empty, but renews 2026-11-07

    clock.t = Date.parse("2026-11-05T12:00:00Z");
    await hostingTick(e, d);
    view = (await operatorView(e)).body as { boxes: { state: string }[]; servers: unknown[]; alerts: { kind: string; message: string }[] };
    expect(view.boxes[0].state).toBe("flagged");
    const flagged = (view as unknown as { alerts: { kind: string; message: string }[] }).alerts.find((a) => a.kind === "flagged")!;
    expect(flagged.message).toContain("Cancel it at OVH before it renews on 2026-11-07");

    // A deploy before Sean cancels it brings the box back rather than buying another.
    await invited(e, d, RIDER2);
    const r = await deploy(e, d, RIDER2, { name: "back", type: "mxbserver", region: "us-east" });
    expect((r.body as { server: { state: string } }).server.state).toBe("ready");
    expect(((await operatorView(e)).body as { boxes: { state: string }[] }).boxes[0].state).toBe("ready");
    expect(id).toBeTruthy();

    expect((await operatorBox(e, d, boxId, "cancelled")).status).toBe(409); // a server is on it
  });

  it("drains the emptiest box when the tenants fit on one fewer", async () => {
    const e = env();
    const { ovh, deliver } = fakeOvh();
    const clock = { t: Date.parse("2026-10-07T10:00:00Z") };
    const d = deps(ovh, fakeFetch(), clock);
    await addTrack(e, d, { pool: "native", name: "Club", url: "https://example.com/c.pkz", sha256: "a".repeat(64) });
    const riders = Array.from({ length: 5 }, (_, i) => `7656119000000020${i}`);
    for (const r of riders) await invited(e, d, r);
    const ids = [];
    for (const r of riders) ids.push(await serverId(await deploy(e, d, r, { name: r, type: "mxbserver", region: "us-east" })));
    expect(ovh.orderVps).toHaveBeenCalledTimes(2);
    deliver(101, "vps-a.vps.ovh.us");
    deliver(102, "vps-bb.vps.ovh.us");
    await hostingTick(e, d);
    clock.t += 3 * 60 * 1000;
    await hostingTick(e, d);
    const boxes = ((await operatorView(e)).body as { boxes: { id: string }[] }).boxes;
    for (const b of boxes) {
      await enrollBox(e, d, b.id, { slots: [1, 2, 3, 4].map((i) => ({ index: i, gamePort: 54209 + i, token: `t${i}.${"f".repeat(64)}` })) });
    }
    await ownerDelete(e, d, riders[0], ids[0]);
    await hostingTick(e, d);
    const states = ((await operatorView(e)).body as { boxes: { state: string; slotsUsed: number }[] }).boxes.map((b) => [b.state, b.slotsUsed]);
    expect(states).toContainEqual(["draining", 1]);
  });
});
