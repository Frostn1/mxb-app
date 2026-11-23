import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Track uploads live in the backend (src-tauri/src/uploads.rs), not in any component. This
 * module is the app-level mirror of them: it outlives every tab, server page and view, so a
 * component unmounting can never stop or forget an upload. Only `cancelUpload` (or the app
 * exiting) does.
 */

export type UploadStatus = "checking" | "uploading" | "retrying" | "installing" | "done" | "error" | "cancelled";

export interface Upload {
  id: string;
  serverId: string;
  serverName: string;
  path: string;
  fileName: string;
  bytes: number;
  sent: number;
  /** Bytes per second. */
  speed: number;
  attempt: number;
  status: UploadStatus;
  error: string | null;
}

export const isActive = (u: Upload) => !["done", "error", "cancelled"].includes(u.status);

export const percent = (u: Upload) => (u.bytes > 0 ? Math.min(100, Math.floor((u.sent / u.bytes) * 100)) : 0);

let uploads: Upload[] = [];
const listeners = new Set<() => void>();
const settled = new Set<(u: Upload) => void>();

function emit() {
  for (const l of listeners) l();
}

function apply(next: Upload) {
  const known = uploads.find((u) => u.id === next.id);
  uploads = known ? uploads.map((u) => (u.id === next.id ? next : u)) : [...uploads, next];
  emit();
  if (!isActive(next) && (!known || isActive(known))) for (const cb of settled) cb(next);
}

export const getUploads = () => uploads;

export function subscribeUploads(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** Be told when an upload finishes (done, failed or cancelled), from any screen. */
export function onUploadSettled(cb: (u: Upload) => void) {
  settled.add(cb);
  return () => void settled.delete(cb);
}

/** The current uploads, re-rendering the caller as they change. */
export const useUploads = () => useSyncExternalStore(subscribeUploads, getUploads);

let started: Promise<void> | null = null;
let unlisten: UnlistenFn[] = [];

/** What to ask before quitting with uploads running; the backend blocks the close until answered. */
export const quitQuestion = (n: number) =>
  `${n} upload${n === 1 ? " is" : "s are"} still running. Quit anyway and cancel ${n === 1 ? "it" : "them"}?`;

export async function handleQuitRequested(confirm: (question: string) => boolean = (q) => window.confirm(q)) {
  const running = uploads.filter(isActive).length;
  if (running === 0 || confirm(quitQuestion(running))) await invoke("quit_app");
}

/** Start listening to the backend and load what is already running. Safe to call repeatedly. */
export function initUploads(): Promise<void> {
  started ??= (async () => {
    unlisten = await Promise.all([
      listen<Upload>("upload-update", (event) => apply(event.payload)),
      listen("quit-requested", () => void handleQuitRequested()),
    ]);
    const existing = await invoke<Upload[]>("upload_list");
    // Anything an event already delivered is newer than this list.
    for (const u of existing) if (!uploads.some((k) => k.id === u.id)) apply(u);
  })();
  return started;
}

export async function startTrackUpload(serverId: string, path: string): Promise<Upload> {
  await initUploads();
  const upload = await invoke<Upload>("upload_start", { id: serverId, path });
  if (!uploads.some((u) => u.id === upload.id)) apply(upload);
  return upload;
}

export async function cancelUpload(id: string): Promise<void> {
  await invoke("upload_cancel", { id });
}

export async function dismissUpload(id: string): Promise<void> {
  await invoke("upload_dismiss", { id });
  uploads = uploads.filter((u) => u.id !== id);
  emit();
}

/** Run a failed or cancelled upload again, from the start. */
export async function retryUpload(u: Upload): Promise<Upload> {
  await dismissUpload(u.id);
  return startTrackUpload(u.serverId, u.path);
}

/** Test seam: forget everything, including the backend listeners. */
export function resetUploads() {
  for (const off of unlisten) off();
  unlisten = [];
  started = null;
  uploads = [];
  listeners.clear();
  settled.clear();
}
