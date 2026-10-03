import { invoke } from "@tauri-apps/api/core";
import { parseCuts, parseOutline, parseRecentCuts, type CutOutline, type CutsInfo, type CutZone, type RecentCut } from "./cuts";
import type { Timing } from "./events";

export interface Server {
  id: string;
  name: string;
  kind: "native" | "legacy";
  host: string;
  agentTls: boolean;
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
export interface NativeStatus {
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

export interface LegacyStatus {
  kind: "stock";
  game: { running: boolean; pid: number | null; uptime_secs: number; restarts: number };
  port: number;
  server: { name: string | null; track: string | null; maxClients: string | null };
}

export type Status = NativeStatus | LegacyStatus;
export const isLegacyStatus = (status: Status): status is LegacyStatus => "kind" in status && status.kind === "stock";

export type ServerState = "online" | "starting" | "offline" | "unreachable";

export interface StatusReport {
  state: ServerState;
  /** The detail behind the state, for a tooltip. */
  detail: string;
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
  guid?: string;
}

export const blankServer = (): Server => ({
  id: "",
  name: "",
  kind: "native",
  host: "",
  agentTls: false,
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
export const parseLegacyPairing = (blob: string) =>
  invoke<{ host: string; port: number; tls: boolean; token: string }>("legacy_pairing", { blob });

export const serverStatus = (id: string) => invoke<StatusReport>("server_status", { id });

export async function serverRiders(id: string): Promise<Rider[]> {
  const body = await invoke<{ riders?: Rider[] }>("server_riders", { id });
  return body.riders ?? [];
}

/** The admin `/v1/cuts`; null when the server is too old to have it. */
export const serverCuts = async (id: string): Promise<CutsInfo | null> => parseCuts(await invoke<unknown>("server_cuts", { id }));

/** One track's outline from the admin `/v1/cuts/outline` (built from its TRH, no riding needed). */
export const serverCutOutline = async (id: string, track: string): Promise<CutOutline> => parseOutline(await invoke<unknown>("server_cut_outline", { id, track }), track);

/** `cuts.recent` of the admin `/v1/events` (empty on an older server). */
export const serverRecentCuts = async (id: string): Promise<RecentCut[]> => parseRecentCuts(await invoke<unknown>("server_cut_events", { id }));

export const serverLogs = (id: string, lines: number) =>
  invoke<string[]>("server_logs", { id, lines });

/** The observe `/timing` feed (no credentials): live laps and split times per rider. */
export const serverTiming = (id: string) => invoke<Timing>("server_timing", { id });

export interface TrackState {
  installed: string[];
  library: string[];
  current: string | null;
  rotation: string[];
}

export const serverTracks = (id: string) => invoke<TrackState>("server_tracks", { id });

export const serverSetTrack = (id: string, track: string) =>
  invoke<Record<string, unknown>>("server_set_track", { id, track });

export const serverSetRotation = (id: string, tracks: string[]) =>
  invoke<Record<string, unknown>>("server_set_rotation", { id, tracks });

export const serverUpdateGithub = (id: string) =>
  invoke<Record<string, unknown>>("server_update_github", { id });

export const serverSession = (id: string, action: "jump" | "advance" | "restart" | "rotate", to?: "practice" | "qualifying" | "warmup" | "race") =>
  invoke<Record<string, unknown>>("server_session", { id, action, to });

export const serverUpload = (id: string, kind: "track" | "version", path: string, version?: string) =>
  invoke<Record<string, unknown>>("server_upload", { id, kind, path, version });

export const inspectTrackUpload = (path: string) =>
  invoke<{ bytes: number; serverTrack: boolean; detail: string; uploadName: string }>("inspect_track_upload", { path });

/** Tauri rejects with the command's error string. Prefer the server's useful message over shell noise. */
export const errorText = (e: unknown) => {
  const text = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  const json = text.match(/\{[^\r\n]*"message"\s*:\s*"(?:\\.|[^"])*"[^\r\n]*\}/)?.[0];
  if (!json) return text;
  try {
    const body = JSON.parse(json) as { message?: unknown };
    return typeof body.message === "string" ? body.message : text;
  } catch {
    return text;
  }
};

// ---- Config editing ----------------------------------------------------------------------

export type FieldKind =
  | { type: "bool" }
  | { type: "int"; min: number; max: number }
  | { type: "float"; min: number; max: number }
  | { type: "text" }
  | { type: "choice"; options: string[] }
  | { type: "racing" }
  | { type: "bikes" };

export interface ConfigField {
  section: string;
  key: string;
  /** "ghosts" | "race" | "events" | "advanced" */
  group: string;
  /** Under "More settings" in its group. */
  advanced: boolean;
  label: string;
  help: string;
  /** What the server does when it's left out. */
  defaultText: string;
  unit: string;
  kind: FieldKind;
}

/** A field's value: null when unset in the file. */
export type FieldValue = boolean | number | string | string[] | CutZone[] | null;

export interface ConfigState {
  text: string;
  sha: string;
  path: string;
  mode: "systemd" | "bare" | "local" | string;
  values: Record<string, FieldValue>;
  fields: ConfigField[];
}

const configCache = new Map<string, { value: ConfigState; at: number }>();
const configLoads = new Map<string, Promise<ConfigState>>();
const CONFIG_CACHE_MS = 5 * 60_000;

export const peekConfig = (id: string) => configCache.get(id)?.value ?? null;

export const configLoad = (id: string, fresh = false): Promise<ConfigState> => {
  const cached = configCache.get(id);
  if (!fresh && cached && Date.now() - cached.at < CONFIG_CACHE_MS) return Promise.resolve(cached.value);
  const pending = configLoads.get(id);
  if (pending) return pending;
  const request = invoke<ConfigState>("config_load", { id })
    .then((value) => { configCache.set(id, { value, at: Date.now() }); return value; })
    .finally(() => configLoads.delete(id));
  configLoads.set(id, request);
  return request;
};

export const configPreview = (base: string, changes: Record<string, FieldValue>) =>
  invoke<{ text: string; diff: string }>("config_preview", { base, changes });

export const configValidate = (id: string, text: string) =>
  invoke<{ ok: boolean; output: string }>("config_validate", { id, text });

export interface ApplyResult {
  result: "applied" | "rolled-back" | "failed" | string;
  backup: string;
  output: string;
}

export const configApply = async (id: string, baseSha: string, text: string) => {
  const result = await invoke<ApplyResult>("config_apply", { id, baseSha, text });
  configCache.delete(id);
  return result;
};

export const testToken = (id: string) => invoke<{ ok: boolean; message: string }>("server_test_token", { id });

export const legacyConfig = (id: string) => invoke<LegacyStatus>("legacy_config", { id });
export const legacyConfigSave = (id: string, name: string, track: string, maxClients: number) =>
  invoke<Record<string, unknown>>("legacy_config_save", { id, name, track, maxClients });
export const legacyProcess = (id: string, action: "start" | "stop" | "restart") =>
  invoke<Record<string, unknown>>("legacy_process", { id, action });

/** `systemctl restart mxbserver` on the server's host, waiting until it answers again. */
export const serverRestartService = (id: string) =>
  invoke<Record<string, unknown>>("server_restart_service", { id });
