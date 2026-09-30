import { describe, expect, it, vi } from "vitest";
import { action, connect, detail, inventory, logs } from "../src/servermanager";
import { d1 } from "./d1sqlite";

function env(hosts = "16-146-6-22.sslip.io"): Env {
  return { DB: d1(), MXB_SERVER_AGENT_HOSTS: hosts } as unknown as Env;
}

const connection = {
  label: "OVH race host",
  provider: "ovh-vps",
  region: "us-west",
  gameEndpoint: "203.0.113.40:54210",
  serverUrl: "https://16-146-6-22.sslip.io",
  adminToken: "ops.secret-control-token-with-enough-length",
  revision: "v0.1.1",
  method: "systemd",
  gamePort: 54210,
};

function reply(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

describe("admin server manager", () => {
  it("is fail-closed on hosts and plaintext credentials", async () => {
    const mock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      reply({ version: "0.1.1", revision: "abc" }),
    );
    const fetcher = mock as unknown as typeof fetch;
    expect((await connect(env(""), connection, fetcher)).status).toBe(400);
    expect((await connect(env(), { ...connection, serverUrl: "http://16-146-6-22.sslip.io" }, fetcher)).status).toBe(400);
    expect(mock).not.toHaveBeenCalled();
  });

  it("probes, stores and lists without ever returning the admin credential", async () => {
    const e = env();
    const mock = vi.fn(async () => reply({ version: "0.1.1", revision: "abc" }));
    const fetcher = mock as unknown as typeof fetch;
    const added = await connect(e, connection, fetcher);
    expect(added.status).toBe(201);
    expect(mock).toHaveBeenCalledWith(
      "https://16-146-6-22.sslip.io/v1/version",
      expect.objectContaining({ headers: expect.objectContaining({ Authorization: "Bearer ops.secret-control-token-with-enough-length" }) }),
    );
    const listed = await inventory(e);
    expect(listed.status).toBe(200);
    expect(JSON.stringify(listed.body)).not.toContain("16-146-6-22.sslip.io");
    expect(JSON.stringify(listed.body)).not.toContain("ops.secret-control-token-with-enough-length");
    expect(listed.body).toMatchObject({
      servers: [{ label: "OVH race host", provider: "ovh-vps", deployment: { revision: "v0.1.1", gamePort: 54210 } }],
    });
  });

  it("refuses a probe that does not answer like an mxbserver admin API", async () => {
    const e = env();
    const mock = vi.fn(async () => reply({ ok: true }));
    const added = await connect(e, connection, mock as unknown as typeof fetch);
    expect(added.status).toBe(409);
  });

  it("aggregates status and session, and routes only the fixed action vocabulary", async () => {
    const e = env();
    const connectFetch = vi.fn(async () => reply({ version: "0.1.1", revision: "abc" })) as unknown as typeof fetch;
    const added = await connect(e, connection, connectFetch);
    const id = (added.body as { id: string }).id;
    const mock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/v1/server")) return reply({ mode: "native", session: "race", version: "0.1.1" });
      if (url.endsWith("/v1/session")) return reply({ track: "RedBud", stage: { kind: "running", session: "race" } });
      if (url.endsWith("/v1/session/jump")) return reply({ entered: { kind: "running", session: "race" } });
      if (url.endsWith("/v1/restart")) return reply({ restarting: true });
      if (url.endsWith("/v1/config/validate")) return reply({ dry_run: true, applicable: true, changes: [] });
      if (url.endsWith("/v1/config/write")) return reply({ written: true, restarting: true, backup: "server.toml.bak-1" });
      return reply({ method: init?.method });
    });
    const fetcher = mock as unknown as typeof fetch;

    expect((await detail(e, id, fetcher)).body).toMatchObject({
      status: { mode: "native" },
      session: { track: "RedBud" },
    });
    expect((await action(e, id, { action: "session", session: { action: "jump", to: "race" } }, fetcher)).status).toBe(200);
    expect((await action(e, id, { action: "restart", drainSeconds: 5 }, fetcher)).status).toBe(200);
    expect((await action(e, id, { action: "config_validate" }, fetcher)).status).toBe(200);
    expect((await action(e, id, { action: "config_write", content: "[server]\nname='x'\n" }, fetcher)).status).toBe(200);
    expect((await action(e, id, { action: "shell", command: "rm -rf /" }, fetcher)).status).toBe(400);
    expect(mock.mock.calls.some(([url]) => String(url).includes("shell"))).toBe(false);
  });

  it("refuses config_write without content and unknown session steps", async () => {
    const e = env();
    const connectFetch = vi.fn(async () => reply({ version: "0.1.1", revision: "abc" })) as unknown as typeof fetch;
    const added = await connect(e, connection, connectFetch);
    const id = (added.body as { id: string }).id;
    const mock = vi.fn(async () => reply({}));
    const fetcher = mock as unknown as typeof fetch;
    expect((await action(e, id, { action: "config_write" }, fetcher)).status).toBe(400);
    expect((await action(e, id, { action: "session", session: { action: "nuke" } }, fetcher)).status).toBe(400);
    expect(mock).not.toHaveBeenCalled();
  });

  it("proxies log tailing to the server's own /v1/logs", async () => {
    const e = env();
    const connectFetch = vi.fn(async () => reply({ version: "0.1.1", revision: "abc" })) as unknown as typeof fetch;
    const added = await connect(e, connection, connectFetch);
    const id = (added.body as { id: string }).id;
    const mock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => reply({ unit: "mxbserver", lines: ["a", "b"] }));
    const result = await logs(e, id, mock as unknown as typeof fetch);
    expect(result.status).toBe(200);
    expect(mock.mock.calls.at(-1)?.[0]).toBe("https://16-146-6-22.sslip.io/v1/logs");
  });
});
