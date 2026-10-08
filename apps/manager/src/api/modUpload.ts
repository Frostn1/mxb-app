import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Mod uploads to the mxbsecure catalogue. The Rust side (`src-tauri/src/modupload.rs`) talks to
 * the control plane's `/v1/uploads` and streams the file's parts straight to R2; this module
 * only carries metadata and progress.
 */

/** The catalogue's types (`control-plane/src/mirror.ts:58` `ASSET_TYPES`). */
export const MOD_KINDS = ["paints", "bikes", "liveries", "kits", "tracks", "other"] as const;
export type ModKind = (typeof MOD_KINDS)[number];

export type Visibility = "public" | "unlisted";

/** What the rider fills in (`uploads.ts:97` `parseOpen`). */
export interface UploadMeta {
  title: string;
  type: ModKind;
  description: string;
  bike: string;
  visibility: Visibility;
  version: string | null;
  notes: string | null;
  /** A new version of this mod of yours; null for a new mod. */
  assetId: number | null;
}

export interface PickedFile {
  path: string;
  filename: string;
  size: number;
  /** `pkz`, `zip` or `pnt`; null for anything the control plane won't take. */
  kind: string | null;
}

export type UploadPhase =
  | "hashing"
  | "uploading"
  | "paused"
  | "completing"
  | "checking"
  | "live"
  | "rejected"
  | "failed";

export interface UploadJob {
  key: string;
  uploadId: string | null;
  path: string;
  filename: string;
  size: number;
  meta: UploadMeta;
  phase: UploadPhase;
  /** Bytes sent (or hashed, while `hashing`). */
  sent: number;
  error: string | null;
  assetId: number | null;
  startedAt: number;
}

/** One of the rider's mods (`uploads.ts:378` `myMods`). */
export interface MyMod {
  id: number;
  title: string;
  modType: string;
  visibility: Visibility;
  /** `active` | `hidden` (moderation) | `removed` (moderation). */
  state: string;
  modified: string;
  currentVersion: number | null;
  /** Open reports. */
  reports: number;
}

/** An upload the control plane has open, checking or rejected. */
export interface MyUpload {
  id: string;
  assetId: number | null;
  filename: string;
  size: number;
  state: string;
  error: string | null;
  createdAt: number;
}

/** `uploads.ts:35` `QUOTA`. */
export interface Quota {
  openSessions: number;
  uploadsPerDay: number;
  bytesPerDay: number;
  storageBytes: number;
}

export interface MyMods {
  mods: MyMod[];
  uploads: MyUpload[];
  quota: Quota;
}

export interface ModEdit {
  title?: string;
  description?: string;
  bike?: string;
  visibility?: Visibility;
}

export const inspectUploadFile = (path: string) => invoke<PickedFile>("mod_upload_inspect", { path });
export const startUpload = (path: string, meta: UploadMeta) => invoke<UploadJob>("mod_upload_start", { path, meta });
export const uploadJobs = () => invoke<UploadJob[]>("mod_upload_jobs");
export const pauseUpload = (key: string) => invoke<void>("mod_upload_pause", { key });
export const resumeUpload = (key: string) => invoke<void>("mod_upload_resume", { key });
export const cancelUpload = (key: string) => invoke<void>("mod_upload_cancel", { key });
export const dismissUpload = (key: string) => invoke<void>("mod_upload_dismiss", { key });
export const myMods = () => invoke<MyMods>("my_mods");
export const editMod = (assetId: number, edit: ModEdit) => invoke<void>("mod_edit", { assetId, edit });
/** The public page's description and bikes, to fill the edit form. Fails until a version is live. */
export const modDetails = (assetId: number) =>
  invoke<{ description: string; bike: string[] }>("mod_details", { assetId });
export const deleteMod =(assetId: number) => invoke<void>("mod_delete", { assetId });

export function onUploadProgress(cb: (job: UploadJob) => void): Promise<UnlistenFn> {
  return listen<UploadJob>("mod-upload", (e) => cb(e.payload));
}
