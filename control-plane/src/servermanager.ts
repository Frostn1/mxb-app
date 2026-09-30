import { isPublicAgentUrl } from "./validate";

/**
 * Proxies the web admin panel straight to a connected mxbserver's own admin API
 * (crates/mxbserver/src/admin) over HTTPS. There is no agent in front of it any more —
 * mxb-agent is a dead project — so every route here is one of mxbserver's own `/v1/*` paths,
 * called with the bearer token stored for that row.
 */

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
  server_url: string;
  admin_token: string;
  deployment_revision: string;
  deployment_method: "docker-compose" | "systemd";
  game_port: number;
}

const MAX_ADMIN_JSON = 256 * 1024;
const PROVIDERS = new Set(["aws-lightsail", "ovh-vps"]);
const METHODS = new Set(["docker-compose", "systemd"]);
// mxbserver's own admin routes (crates/mxbserver/src/admin/mod.rs). Kept as an allowlist so a
// stored row can never be used to call anything else on the box.
const CONTROL_ACTIONS = new Set(["restart", "config_validate", "config_write", "session"]);

function text(value: unknown, maximum: number): string | null {
  if (typeof value !== "string") return null;
  const clean = value.trim();
  if (!clean || clean.length > maximum || /[\u0000-\u001f\u007f]/.test(clean)) return null;
  return clean;
}

function allowedServer(url: string, env: Env): boolean {
  if (!isPublicAgentUrl(url)) return false;
  const parsed = new URL(url);
  // The bearer controls the game process and its config file, so it may not cross plaintext.
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
    `SELECT id, label, provider, region, lifecycle, game_endpoint, server_url, admin_token,
            deployment_revision, deployment_method, game_port
       FROM managed_servers WHERE id = ?`,
  )
    .bind(id)
    .first<ManagedRow>();
}

async function limitedJson(response: Response): Promise<unknown> {
  const announced = Number(response.headers.get("Content-Length") ?? "0");
  if (announced > MAX_ADMIN_JSON) throw new Error("mxbserver's response was too large");
  if (!response.body) return {};
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const next = await reader.read();
    if (next.done) break;
    size += next.value.byteLength;
    if (size > MAX_ADMIN_JSON) {
      await reader.cancel();
      throw new Error("mxbserver's response was too large");
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

/** `path` is one of mxbserver's own `/v1/*` admin routes. */
async function call(
  env: Env,
  server: ManagedRow,
  path: string,
  init: RequestInit,
  fetchImpl: typeof fetch,
): Promise<ManagerResult> {
  if (!allowedServer(server.server_url, env)) {
    return { status: 502, body: { error: "the stored server host is no longer allowlisted" } };
  }
  let response: Response;
  try {
    response = await fetchImpl(`${server.server_url}${path}`, {
      ...init,
      redirect: "manual",
      headers: {
        Authorization: `Bearer ${server.admin_token}`,
        ...(init.headers ?? {}),
      },
    });
  } catch {
    return { status: 502, body: { error: "couldn't reach that server's admin API" } };
  }
  if (response.status >= 300 && response.status < 400) {
    return { status: 502, body: { error: "the server tried to redirect the request" } };
  }
  try {
    const body = await limitedJson(response);
    return response.ok
      ? { status: 200, body }
      : {
          status: response.status >= 400 && response.status < 600 ? response.status : 502,
          body: { error: (body as { message?: unknown; error?: unknown })?.message ?? (body as { error?: unknown })?.error ?? "the server refused the request" },
        };
  } catch {
    return { status: 502, body: { error: "the server returned an unreadable response" } };
  }
}

export async function inventory(env: Env): Promise<ManagerResult> {
  const rows = await env.DB.prepare(
    `SELECT id, label, provider, region, lifecycle, game_endpoint, deployment_revision,
            deployment_method, game_port
       FROM managed_servers ORDER BY label COLLATE NOCASE, id`,
  ).all<Omit<ManagedRow, "server_url" | "admin_token">>();
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
  const serverUrl = text(input.serverUrl, 256)?.replace(/\/$/, "") ?? null;
  const adminToken = text(input.adminToken, 512);
  const revision = text(input.revision, 120);
  const method = text(input.method, 30);
  const gamePort = Number(input.gamePort);
  if (!label || !provider || !PROVIDERS.has(provider) || !region || !serverUrl || !adminToken || !revision || !method || !METHODS.has(method)) {
    return { status: 400, body: { error: "the server connection details are incomplete" } };
  }
  if (adminToken.length < 32) return { status: 400, body: { error: "the admin token must be at least 32 characters" } };
  if (!Number.isInteger(gamePort) || gamePort < 1 || gamePort > 65535) {
    return { status: 400, body: { error: "gamePort must be between 1 and 65535" } };
  }
  if (input.gameEndpoint !== undefined && input.gameEndpoint !== "" && !gameEndpoint) {
    return { status: 400, body: { error: "gameEndpoint is invalid" } };
  }
  if (!allowedServer(serverUrl, env)) {
    return { status: 400, body: { error: "that HTTPS host is not in MXB_SERVER_AGENT_HOSTS" } };
  }
  const probe = await call(
    env,
    { id: "probe", label, provider: provider as ManagedRow["provider"], region, lifecycle: "unknown", game_endpoint: gameEndpoint, server_url: serverUrl, admin_token: adminToken, deployment_revision: revision, deployment_method: method as ManagedRow["deployment_method"], game_port: gamePort },
    "/v1/version",
    { method: "GET" },
    fetchImpl,
  );
  if (probe.status !== 200) return probe;
  const version = probe.body as { version?: unknown; revision?: unknown; build_id?: unknown };
  if (typeof version.version !== "string" || typeof version.revision !== "string") {
    return { status: 409, body: { error: "that endpoint did not answer like an mxbserver admin API" } };
  }
  const id = crypto.randomUUID();
  const now = Date.now();
  try {
    await env.DB.prepare(
      `INSERT INTO managed_servers
         (id, label, provider, region, lifecycle, game_endpoint, server_url, admin_token,
          deployment_revision, deployment_method, game_port, created_at, updated_at)
       VALUES (?, ?, ?, ?, 'running', ?, ?, ?, ?, ?, ?, ?, ?)`,
    )
      .bind(id, label, provider, region, gameEndpoint, serverUrl, adminToken, revision, method, gamePort, now, now)
      .run();
  } catch {
    return { status: 409, body: { error: "that server is already connected" } };
  }
  return { status: 201, body: { ok: true, id, version } };
}

export async function detail(env: Env, id: string, fetchImpl: typeof fetch = fetch): Promise<ManagerResult> {
  const server = await row(env, id);
  if (!server) return { status: 404, body: { error: "no such managed server" } };
  const status = await call(env, server, "/v1/server", { method: "GET" }, fetchImpl);
  if (status.status !== 200) return status;
  const body = status.body as { mode?: unknown; session?: unknown };
  const native = body.mode === "native" && body.session !== "relay";
  const session = native ? await call(env, server, "/v1/session", { method: "GET" }, fetchImpl) : null;
  return {
    status: 200,
    body: {
      id: server.id,
      label: server.label,
      provider: server.provider,
      region: server.region,
      gameEndpoint: server.game_endpoint,
      deployment: { revision: server.deployment_revision, method: server.deployment_method, gamePort: server.game_port },
      status: status.body,
      session: session && session.status === 200 ? session.body : null,
    },
  };
}

/** `input.action` is one of `restart`, `config_validate`, `config_write` or `session` (whose
 * `input.session.action` picks the `/v1/session/*` route). Everything else is refused before it
 * reaches the network. */
export async function action(
  env: Env,
  id: string,
  input: Record<string, unknown>,
  fetchImpl: typeof fetch = fetch,
): Promise<ManagerResult> {
  const server = await row(env, id);
  if (!server) return { status: 404, body: { error: "no such managed server" } };
  const name = text(input.action, 30);
  if (!name || !CONTROL_ACTIONS.has(name)) return { status: 400, body: { error: "unsupported server action" } };
  if (name === "restart") {
    const drainSeconds = Number.isInteger(input.drainSeconds) ? (input.drainSeconds as number) : 0;
    if (drainSeconds < 0 || drainSeconds > 3_600) return { status: 400, body: { error: "drainSeconds must be 0-3600" } };
    return call(env, server, "/v1/restart", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ drain_seconds: drainSeconds, reason: text(input.reason, 200) ?? "" }) }, fetchImpl);
  }
  if (name === "config_validate") {
    return call(env, server, "/v1/config/validate", { method: "POST" }, fetchImpl);
  }
  if (name === "config_write") {
    const content = typeof input.content === "string" ? input.content : "";
    if (!content.trim() || content.length > 16_000) return { status: 400, body: { error: "content is required" } };
    const drainSeconds = Number.isInteger(input.drainSeconds) ? (input.drainSeconds as number) : 0;
    return call(env, server, "/v1/config/write", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ content, drain_seconds: drainSeconds }) }, fetchImpl);
  }
  // session
  const session = input.session;
  if (!session || typeof session !== "object" || Array.isArray(session)) return { status: 400, body: { error: "session needs an action" } };
  const step = text((session as Record<string, unknown>).action, 20);
  const routes: Record<string, string> = {
    advance: "/v1/session/advance",
    restart: "/v1/session/restart",
    jump: "/v1/session/jump",
    next: "/v1/session/next",
    rotate: "/v1/session/rotate",
  };
  const path = step ? routes[step] : undefined;
  if (!path) return { status: 400, body: { error: "session.action must be advance, restart, jump, next or rotate" } };
  const { action: _drop, ...rest } = session as Record<string, unknown>;
  return call(env, server, path, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(rest) }, fetchImpl);
}

export async function logs(env: Env, id: string, fetchImpl: typeof fetch = fetch): Promise<ManagerResult> {
  const server = await row(env, id);
  if (!server) return { status: 404, body: { error: "no such managed server" } };
  return call(env, server, "/v1/logs", { method: "GET" }, fetchImpl);
}
