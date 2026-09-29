import { isPublicAgentUrl } from "./validate";

export interface ManagerResult {
  status: number;
  body: unknown;
}

interface ManagedRow {
  id: string;
  label: string;
  provider: "aws-lightsail" | "ovh-vps";
  region: string;
  lifecycle: "running" | "planned" | "retired" | "unknown";
  game_endpoint: string | null;
  agent_url: string;
  agent_token: string;
  deployment_revision: string;
  deployment_method: "docker-compose" | "systemd";
  game_port: number;
}

const MAX_AGENT_JSON = 256 * 1024;
const PROVIDERS = new Set(["aws-lightsail", "ovh-vps"]);
const METHODS = new Set(["docker-compose", "systemd"]);
const ACTIONS = new Set(["start", "stop", "restart"]);

function text(value: unknown, maximum: number): string | null {
  if (typeof value !== "string") return null;
  const clean = value.trim();
  if (!clean || clean.length > maximum || /[\u0000-\u001f\u007f]/.test(clean)) return null;
  return clean;
}

function allowedAgent(url: string, env: Env): boolean {
  if (!isPublicAgentUrl(url)) return false;
  const parsed = new URL(url);
  // The bearer controls a process and software installation, so it may not cross plaintext.
  if (parsed.protocol !== "https:") return false;
  const allowed = new Set(
    (env.MXB_SERVER_AGENT_HOSTS ?? "")
      .split(/[\s,]+/)
      .map((host) => host.trim().toLowerCase())
      .filter(Boolean),
  );
  return allowed.has(parsed.hostname.toLowerCase());
}

async function row(env: Env, id: string): Promise<ManagedRow | null> {
  return env.DB.prepare(
    `SELECT id, label, provider, region, lifecycle, game_endpoint, agent_url, agent_token,
            deployment_revision, deployment_method, game_port
       FROM managed_servers WHERE id = ?`,
  )
    .bind(id)
    .first<ManagedRow>();
}

async function limitedJson(response: Response): Promise<unknown> {
  const announced = Number(response.headers.get("Content-Length") ?? "0");
  if (announced > MAX_AGENT_JSON) throw new Error("agent response was too large");
  if (!response.body) return {};
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const next = await reader.read();
    if (next.done) break;
    size += next.value.byteLength;
    if (size > MAX_AGENT_JSON) {
      await reader.cancel();
      throw new Error("agent response was too large");
    }
    chunks.push(next.value);
  }
  const bytes = new Uint8Array(size);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.byteLength;
  }
  const decoded = new TextDecoder().decode(bytes);
  return decoded ? JSON.parse(decoded) : {};
}

async function call(
  env: Env,
  server: ManagedRow,
  path: string,
  init: RequestInit,
  fetchImpl: typeof fetch,
): Promise<ManagerResult> {
  if (!allowedAgent(server.agent_url, env)) {
    return { status: 502, body: { error: "the stored server agent host is no longer allowlisted" } };
  }
  let response: Response;
  try {
    response = await fetchImpl(`${server.agent_url}${path}`, {
      ...init,
      redirect: "manual",
      headers: {
        Authorization: `Bearer ${server.agent_token}`,
        ...(init.headers ?? {}),
      },
    });
  } catch {
    return { status: 502, body: { error: "couldn't reach that server agent" } };
  }
  if (response.status >= 300 && response.status < 400) {
    return { status: 502, body: { error: "the server agent tried to redirect the request" } };
  }
  try {
    const body = await limitedJson(response);
    return response.ok
      ? { status: 200, body }
      : {
          status: response.status >= 400 && response.status < 600 ? response.status : 502,
          body: { error: (body as { error?: unknown })?.error ?? "the server agent refused the request" },
        };
  } catch {
    return { status: 502, body: { error: "the server agent returned an unreadable response" } };
  }
}

export async function inventory(env: Env): Promise<ManagerResult> {
  const rows = await env.DB.prepare(
    `SELECT id, label, provider, region, lifecycle, game_endpoint, deployment_revision,
            deployment_method, game_port
       FROM managed_servers ORDER BY label COLLATE NOCASE, id`,
  ).all<Omit<ManagedRow, "agent_url" | "agent_token">>();
  return {
    status: 200,
    body: {
      generatedAt: Date.now(),
      servers: rows.results.map((server) => ({
        id: server.id,
        label: server.label,
        provider: server.provider,
        region: server.region,
        lifecycle: server.lifecycle,
        ...(server.game_endpoint ? { gameEndpoint: server.game_endpoint } : {}),
        deployment: {
          repository: "Frostn1/mxbserver",
          revision: server.deployment_revision,
          method: server.deployment_method,
          gamePort: server.game_port,
          externalConfiguration: [],
        },
      })),
    },
  };
}

export async function connect(
  env: Env,
  input: Record<string, unknown>,
  fetchImpl: typeof fetch = fetch,
): Promise<ManagerResult> {
  const label = text(input.label, 80);
  const provider = text(input.provider, 30);
  const region = text(input.region, 80);
  const gameEndpoint = input.gameEndpoint === undefined || input.gameEndpoint === "" ? null : text(input.gameEndpoint, 180);
  const agentUrl = text(input.agentUrl, 256)?.replace(/\/$/, "") ?? null;
  const agentToken = text(input.agentToken, 512);
  const revision = text(input.revision, 120);
  const method = text(input.method, 30);
  const gamePort = Number(input.gamePort);
  if (!label || !provider || !PROVIDERS.has(provider) || !region || !agentUrl || !agentToken || !revision || !method || !METHODS.has(method)) {
    return { status: 400, body: { error: "the server connection details are incomplete" } };
  }
  if (agentToken.length < 32) return { status: 400, body: { error: "the agent token must be at least 32 characters" } };
  if (!Number.isInteger(gamePort) || gamePort < 1 || gamePort > 65535) {
    return { status: 400, body: { error: "gamePort must be between 1 and 65535" } };
  }
  if (input.gameEndpoint !== undefined && input.gameEndpoint !== "" && !gameEndpoint) {
    return { status: 400, body: { error: "gameEndpoint is invalid" } };
  }
  if (!allowedAgent(agentUrl, env)) {
    return { status: 400, body: { error: "that HTTPS agent host is not in MXB_SERVER_AGENT_HOSTS" } };
  }
  const probe = await call(
    env,
    { id: "probe", label, provider: provider as ManagedRow["provider"], region, lifecycle: "unknown", game_endpoint: gameEndpoint, agent_url: agentUrl, agent_token: agentToken, deployment_revision: revision, deployment_method: method as ManagedRow["deployment_method"], game_port: gamePort },
    "/capabilities",
    { method: "GET" },
    fetchImpl,
  );
  if (probe.status !== 200) return probe;
  const capabilities = probe.body as { apiVersion?: unknown; kind?: unknown };
  if (capabilities.apiVersion !== 1 || !["native", "stock"].includes(String(capabilities.kind))) {
    return { status: 409, body: { error: "that endpoint is not a compatible mxb-agent" } };
  }
  const id = crypto.randomUUID();
  const now = Date.now();
  try {
    await env.DB.prepare(
      `INSERT INTO managed_servers
         (id, label, provider, region, lifecycle, game_endpoint, agent_url, agent_token,
          deployment_revision, deployment_method, game_port, created_at, updated_at)
       VALUES (?, ?, ?, ?, 'running', ?, ?, ?, ?, ?, ?, ?, ?)`,
    )
      .bind(id, label, provider, region, gameEndpoint, agentUrl, agentToken, revision, method, gamePort, now, now)
      .run();
  } catch {
    return { status: 409, body: { error: "that server agent is already connected" } };
  }
  return { status: 201, body: { ok: true, id, capabilities } };
}

export async function detail(env: Env, id: string, fetchImpl: typeof fetch = fetch): Promise<ManagerResult> {
  const server = await row(env, id);
  if (!server) return { status: 404, body: { error: "no such managed server" } };
  const [capabilities, status, tracks] = await Promise.all([
    call(env, server, "/capabilities", { method: "GET" }, fetchImpl),
    call(env, server, "/status", { method: "GET" }, fetchImpl),
    call(env, server, "/tracks", { method: "GET" }, fetchImpl),
  ]);
  const failed = [capabilities, status, tracks].find((result) => result.status !== 200);
  if (failed) return failed;
  return {
    status: 200,
    body: {
      id: server.id,
      label: server.label,
      provider: server.provider,
      region: server.region,
      gameEndpoint: server.game_endpoint,
      deployment: { revision: server.deployment_revision, method: server.deployment_method, gamePort: server.game_port },
      capabilities: capabilities.body,
      status: status.body,
      tracks: (tracks.body as { tracks?: unknown }).tracks ?? [],
    },
  };
}

export async function action(
  env: Env,
  id: string,
  input: Record<string, unknown>,
  fetchImpl: typeof fetch = fetch,
): Promise<ManagerResult> {
  const server = await row(env, id);
  if (!server) return { status: 404, body: { error: "no such managed server" } };
  const name = text(input.action, 30);
  if (!name) return { status: 400, body: { error: "action is required" } };
  if (ACTIONS.has(name)) return call(env, server, `/${name}`, { method: "POST" }, fetchImpl);
  if (name === "config") {
    const patch = input.patch;
    if (!patch || typeof patch !== "object" || Array.isArray(patch)) return { status: 400, body: { error: "config needs a patch" } };
    return call(env, server, "/config", { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify(patch) }, fetchImpl);
  }
  if (name === "session") {
    const session = input.session;
    if (!session || typeof session !== "object" || Array.isArray(session)) return { status: 400, body: { error: "session needs an action" } };
    return call(env, server, "/session", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(session) }, fetchImpl);
  }
  return { status: 400, body: { error: "unsupported server action" } };
}

export async function upload(
  env: Env,
  id: string,
  kind: "track" | "version",
  request: Request,
  fetchImpl: typeof fetch = fetch,
): Promise<ManagerResult> {
  const server = await row(env, id);
  if (!server) return { status: 404, body: { error: "no such managed server" } };
  if (!request.body) return { status: 400, body: { error: "upload body is required" } };
  const digest = request.headers.get("X-Content-SHA256") ?? "";
  if (!/^[a-f0-9]{64}$/i.test(digest)) return { status: 400, body: { error: "X-Content-SHA256 is required" } };
  const headers: Record<string, string> = { "Content-Type": "application/octet-stream", "X-Content-SHA256": digest };
  if (kind === "track") {
    const filename = request.headers.get("X-Filename") ?? "";
    if (!/^[^\\/\r\n]{1,128}\.pkz$/i.test(filename) || filename.startsWith(".")) return { status: 400, body: { error: "X-Filename must be a plain .pkz file name" } };
    headers["X-Filename"] = filename;
  } else {
    const version = request.headers.get("X-Version") ?? "";
    if (!version || version.length > 80 || /[\r\n]/.test(version)) return { status: 400, body: { error: "X-Version is required" } };
    headers["X-Version"] = version;
  }
  const result = await call(env, server, kind === "track" ? "/tracks" : "/version", { method: "PUT", headers, body: request.body }, fetchImpl);
  if (kind === "version" && result.status === 200) {
    await env.DB.prepare("UPDATE managed_servers SET deployment_revision = ?, updated_at = ? WHERE id = ?")
      .bind(headers["X-Version"], Date.now(), id)
      .run();
  }
  return result;
}
