import { invoke } from "@tauri-apps/api/core";
import type { ServerView, StatusReport } from "./api";

/** The control plane's view of a hosted server (control-plane/src/hosting.ts `serverView`). */
export interface HostedServer {
  id: string;
  name: string;
  type: "mxbserver" | "legacy";
  region: string;
  regionLabel: string;
  state: "provisioning" | "installing" | "ready" | "failed";
  progress: { step: number; steps: string[]; since: number; note: string | null };
  address: string | null;
  settings: { track: string | null; bikeSet: string | null; maxRiders: number };
  options: { tracks: { id: string; name: string }[]; bikeSets: { id: string; name: string }[]; maxRiders: number };
  riders: number;
  idleSince: number | null;
  freedAt: number | null;
  createdAt: number;
  error: string | null;
}

export interface HostedSettings {
  track?: string;
  bikeSet?: string;
  maxRiders?: number;
}

/** Swap a code from servers.mxbsecure.com for a saved hosted server. */
export const hostedClaim = (code: string) => invoke<ServerView>("hosted_claim", { code: code.trim() });

/** The code of an `mxbservers://` link that opened the app, once. */
export const hostedTakePending = () => invoke<string | null>("hosted_take_pending");

export const hostedServer = async (id: string) => (await invoke<{ server: HostedServer }>("hosted_server", { id })).server;

export const hostedSaveSettings = async (id: string, settings: HostedSettings) =>
  (await invoke<{ server: HostedServer }>("hosted_settings", { id, settings })).server;

export const hostedRestart = (id: string) => invoke<{ ok: boolean }>("hosted_restart", { id });

/** A pasted code, or a whole `mxbservers://` link, down to the code. */
export function claimCode(text: string): string {
  const value = text.trim();
  const match = value.match(/^mxbservers:\/\/(?:hosted\/claim\/?\?(?:.*&)?code|connect\/?\?(?:.*&)?claim)=([A-Za-z0-9_-]+)/i);
  return match ? match[1] : value;
}

/** A hosted server's state in the words and colours every other server uses. */
export function hostedReport(server: HostedServer): StatusReport {
  switch (server.state) {
    case "ready":
      return { state: "online", detail: server.address ?? "Ready", status: null };
    case "failed":
      return { state: "unreachable", detail: server.error ?? "Failed", status: null };
    default:
      return { state: "starting", detail: server.progress.note ?? server.progress.steps[server.progress.step - 1] ?? "Starting", status: null };
  }
}


