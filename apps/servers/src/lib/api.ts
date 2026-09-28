import { invoke } from "@tauri-apps/api/core";

export interface Server {
  id: string;
  name: string;
  host: string;
  sshPort: number;
  user: string;
  keyPath: string | null;
  observePort: number;
  adminPort: number | null;
  logPath: string;
  /** On this PC: ports used directly on 127.0.0.1, the log read as a local file, no SSH. */
  local: boolean;
}

export interface ServerView extends Server {
  hasToken: boolean;
}

/** mxbserver's observe `/status` (crates/mxbserver/src/observability.rs). */
export interface Status {
  version: string;
  revision: string;
  build_id: string;
  ready: boolean;
  uptime_seconds: number;
  active_sessions: number;
  client_datagrams_total: number;
  server_datagrams_total: number;
  session: string;
  session_remaining_seconds: number | null;
}

export interface StatusReport {
  ready: boolean;
  status: Status | null;
}

/** One entry of the admin `/v1/riders` (crates/mxbserver/src/admin/mod.rs `rider_json`). */
export interface Rider {
  connection_id: number;
  entity_id: number | null;
  name: string;
  bike: string | null;
  state: string;
  connected_seconds: number;
  laps: number;
  best_lap_seconds: number | null;
  ping_ms: number | null;
}

export const blankServer = (): Server => ({
  id: "",
  name: "",
  host: "",
  sshPort: 22,
  user: "ubuntu",
  keyPath: null,
  observePort: 9809,
  adminPort: null,
  logPath: "/opt/mxbserver/logs/mxbserver.log",
  local: false,
});

export const listServers = () => invoke<ServerView[]>("servers_list");

/** `token`: undefined keeps the saved one, "" removes it. */
export const saveServer = (server: Server, token?: string) =>
  invoke<ServerView>("servers_save", { request: { server, token } });

export const removeServer = (id: string) => invoke<void>("servers_remove", { id });

export const serverStatus = (id: string) => invoke<StatusReport>("server_status", { id });

export async function serverRiders(id: string): Promise<Rider[]> {
  const body = await invoke<{ riders?: Rider[] }>("server_riders", { id });
  return body.riders ?? [];
}

export const serverLogs = (id: string, lines: number) =>
  invoke<string[]>("server_logs", { id, lines });

/** Tauri rejects with the command's error string. */
export const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
