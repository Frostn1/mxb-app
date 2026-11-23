import { invoke } from "@tauri-apps/api/core";

/** Where a ride came from. Game replays are listed as a source but not read yet. */
export type Source = "coach" | "replay";

export interface SessionSummary {
  path: string;
  source: Source;
  /** `YYYY-MM-DDTHH:MM:SS`, local time of the recording. */
  started: string;
  rider: string;
  bikeId: string;
  bikeName: string;
  trackId: string;
  trackName: string;
  duration: number;
  jumps: number;
  bestAir: number;
  topSpeed: number;
  size: number;
}

export interface Jump {
  start: number;
  end: number;
  apex: number;
  height: number;
  length: number;
}

/** One ride, every channel a typed array of the same length. */
export interface Ride {
  summary: SessionSummary;
  suspMaxTravel: [number, number];
  leanKnown: [boolean, boolean];
  stanceKnown: boolean;
  jumps: Jump[];
  count: number;
  t: Float32Array;
  x: Float32Array;
  y: Float32Array;
  z: Float32Array;
  yaw: Float32Array;
  pitch: Float32Array;
  roll: Float32Array;
  steer: Float32Array;
  usedF: Float32Array;
  usedR: Float32Array;
  speed: Float32Array;
  spin: Float32Array;
  throttle: Float32Array;
  brake: Float32Array;
  gear: Float32Array;
  air: Float32Array;
  crashed: Float32Array;
  leanLR: Float32Array;
  leanFB: Float32Array;
  stance: Float32Array;
}

const CHANNELS = [
  "t", "x", "y", "z", "yaw", "pitch", "roll", "steer", "usedF", "usedR", "speed", "spin",
  "throttle", "brake", "gear", "air", "crashed", "leanLR", "leanFB", "stance",
] as const;

function f32(b64: string): Float32Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Float32Array(bytes.buffer, 0, bytes.length >> 2);
}

export function listSessions(): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>("cinema_sessions");
}

export async function loadRide(path: string): Promise<Ride> {
  const raw = await invoke<{
    summary: SessionSummary;
    suspMaxTravel: [number, number];
    leanKnown: [boolean, boolean];
    stanceKnown: boolean;
    jumps: Jump[];
    count: number;
    channels: Record<string, string>;
  }>("cinema_session", { path });
  const ride = {
    summary: raw.summary,
    suspMaxTravel: raw.suspMaxTravel,
    leanKnown: raw.leanKnown,
    stanceKnown: raw.stanceKnown,
    jumps: raw.jumps,
    count: raw.count,
  } as Ride;
  for (const c of CHANNELS) (ride as unknown as Record<string, Float32Array>)[c] = f32(raw.channels[c] ?? "");
  return ride;
}

export interface Ground {
  path: string;
  prefix: string | null;
  name: string;
  lift: number;
}

export function rideGround(path: string): Promise<{ ground: Ground | null; why: string | null }> {
  return invoke("cinema_ground", { path });
}

export interface BlenderInstall {
  path: string;
  version: string;
  tooOld: boolean;
}

export interface CinemaSettings {
  blenderPath: string;
  outputDir: string;
}

export interface SettingsView {
  settings: CinemaSettings;
  found: BlenderInstall[];
  using: BlenderInstall | null;
  output: string;
}

export function getSettings(): Promise<SettingsView> {
  return invoke<SettingsView>("cinema_settings");
}

export function setSettings(settings: CinemaSettings): Promise<SettingsView> {
  return invoke<SettingsView>("cinema_set_settings", { settings });
}

export function checkBlender(path: string): Promise<BlenderInstall | null> {
  return invoke<BlenderInstall | null>("cinema_check_blender", { path });
}

export type RenderKind = "none" | "still" | "clip";
export type CameraPick = "auto" | "chase" | "trackside" | "detail";

export interface ExportRequest {
  path: string;
  start: number;
  end: number;
  mark: number;
  render: RenderKind;
  res: [number, number];
  samples: number;
  camera: CameraPick;
  rider: boolean;
  fx: boolean;
  /** The preview's camera: the shot is framed the way the user left the view. */
  view: { position: [number, number, number]; quaternion: [number, number, number, number]; fov: number; aspect: number } | null;
  /** The still: the game's look, cinematic, or cinematic + realism; from the view or a photographer's camera. */
  look?: "matched" | "cinematic" | "realism";
  shot?: "view" | "photo" | "close" | "tight";
  side?: "sun" | "shade";
  /** 0..1: mud on the bike and dust on the gear (realism), background blur and motion blur (cinematic). */
  mud?: number;
  blur?: number;
  motion?: number;
}

export type JobState = "idle" | "preparing" | "blender" | "done" | "failed" | "cancelled";

export interface JobStatus {
  state: JobState;
  step: string;
  log: string[];
  error: string | null;
  blend: string | null;
  still: string | null;
  clip: string | null;
  folder: string | null;
  elapsed: number;
}

export function startExport(request: ExportRequest): Promise<void> {
  return invoke("cinema_export", { request });
}

export function exportStatus(): Promise<JobStatus> {
  return invoke<JobStatus>("cinema_export_status");
}

export function cancelExport(): Promise<void> {
  return invoke("cinema_export_cancel");
}

export function openBlend(path: string): Promise<void> {
  return invoke("cinema_open_blend", { path });
}

export function readImage(path: string): Promise<string> {
  return invoke<string>("cinema_read_image", { path });
}

export function revealInExplorer(path: string): Promise<void> {
  return invoke("reveal_in_explorer", { path });
}

export function openFolder(path: string): Promise<void> {
  return invoke("open_folder", { path });
}

/** The longest slice a shot is made from, seconds (export.rs `MAX_SPAN_S`). */
export const MAX_SPAN_S = 20;

/** Explorer with the file selected. */
export function revealShot(path: string): Promise<void> {
  return invoke("cinema_reveal", { path });
}
