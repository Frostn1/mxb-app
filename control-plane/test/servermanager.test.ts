import { describe, expect, it, vi } from "vitest";
import { action, connect, detail, inventory, upload } from "../src/servermanager";
import { d1 } from "./d1sqlite";

function env(hosts = "agent.example.com"): Env {
  return { DB: d1(), MXB_SERVER_AGENT_HOSTS: hosts } as unknown as Env;
}

const connection = {
  label: "OVH race host",
  provider: "ovh-vps",
  region: "us-west",
  gameEndpoint: "203.0.113.40:54210",
  agentUrl: "https://agent.example.com",
  agentToken: "secret-control-token-with-32-characters",
  revision: "v0.1.1",
  method: "systemd",
  gamePort: 54210,
};

function agent(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

describe("admin server manager", () => {
  it("is fail-closed on hosts and plaintext credentials", async () => {
    const mock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      agent({ apiVersion: 1, kind: "native" }),
    );
    const fetcher = mock as unknown as typeof fetch;
    expect((await connect(env(""), connection, fetcher)).status).toBe(400);
    expect((await connect(env(), { ...connection, agentUrl: "http://agent.example.com" }, fetcher)).status).toBe(400);
    expect(mock).not.toHaveBeenCalled();
  });

  it("probes, stores and lists without ever returning the agent credential", async () => {
    const e = env();
    const mock = vi.fn(async () => agent({ apiVersion: 1, kind: "native" }));
    const fetcher = mock as unknown as typeof fetch;
    const added = await connect(e, connection, fetcher);
    expect(added.status).toBe(201);
    expect(mock).toHaveBeenCalledWith(
      "https://agent.example.com/capabilities",
      expect.objectContaining({ headers: expect.objectContaining({ Authorization: "Bearer secret-control-token-with-32-characters" }) }),
    );
    const listed = await inventory(e);
    expect(listed.status).toBe(200);
    expect(JSON.stringify(listed.body)).not.toContain("agent.example.com");
    expect(JSON.stringify(listed.body)).not.toContain("secret-control-token-with-32-characters");
    expect(listed.body).toMatchObject({
      servers: [{ label: "OVH race host", provider: "ovh-vps", deployment: { revision: "v0.1.1", gamePort: 54210 } }],
    });
  });

  it("aggregates status and routes only the fixed action vocabulary", async () => {
    const e = env();
    const connectFetch = vi.fn(async () => agent({ apiVersion: 1, kind: "native" })) as unknown as typeof fetch;
    const added = await connect(e, connection, connectFetch);
    const id = (added.body as { id: string }).id;
    const mock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/capabilities")) return agent({ apiVersion: 1, kind: "native", actions: { sessions: true } });
      if (url.endsWith("/status")) return agent({ game: { running: true }, server: { track: "RedBud", bots: 4 } });
      if (url.endsWith("/tracks")) return agent({ tracks: ["RedBud", "Southwick"] });
      if (url.endsWith("/session")) return agent({ entered: { session: "race" } });
      return agent({ method: init?.method });
    });
    const fetcher = mock as unknown as typeof fetch;

    expect((await detail(e, id, fetcher)).body).toMatchObject({ tracks: ["RedBud", "Southwick"], status: { server: { bots: 4 } } });
    expect((await action(e, id, { action: "session", session: { action: "jump", to: "race" } }, fetcher)).status).toBe(200);
    expect((await action(e, id, { action: "shell", command: "rm -rf /" }, fetcher)).status).toBe(400);
    expect(mock.mock.calls.some(([url]) => String(url).includes("shell"))).toBe(false);
  });

  it("streams only named, hashed uploads to fixed agent endpoints", async () => {
    const e = env();
    const mock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      agent({ apiVersion: 1, kind: "native" }),
    );
    const fetcher = mock as unknown as typeof fetch;
    const added = await connect(e, connection, fetcher);
    const id = (added.body as { id: string }).id;
    mock.mockImplementation(async (_input: RequestInfo | URL, init?: RequestInit) => {
      expect(init?.method).toBe("PUT");
      return agent({ ok: true });
    });
    const request = new Request("https://api.example/upload", {
      method: "POST",
      headers: {
        "Content-Type": "application/octet-stream",
        "X-Filename": "RedBud.pkz",
        "X-Content-SHA256": "a".repeat(64),
      },
      body: new Uint8Array([1, 2, 3]),
    });
    expect((await upload(e, id, "track", request, fetcher)).status).toBe(200);
    expect(mock.mock.calls.at(-1)?.[0]).toBe("https://agent.example.com/tracks");
  });
});
