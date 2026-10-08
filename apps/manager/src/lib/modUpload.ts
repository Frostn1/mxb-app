/**
 * Checks for the upload form, run before anything is hashed or sent, so a rider hears "too big"
 * or "pick a bike" at once rather than from the control plane after a long hash. The control
 * plane checks the same things again (`control-plane/src/uploads.ts:97` `parseOpen`).
 */
import type { ModKind, MyMods, PickedFile, UploadJob, UploadMeta } from "../api/modUpload";

/** `uploads.ts:31` `MAX_UPLOAD_BYTES`. */
export const MAX_UPLOAD_BYTES = 2 * 1024 ** 3;
/** `modscan.ts:31` `MAX_PNT_BYTES`. */
export const MAX_PNT_BYTES = 64 * 1024 ** 2;
/** `uploads.ts:111`, `:118`, `:119`, `:132`, `:133`. */
export const MAX_TITLE = 120;
export const MAX_DESCRIPTION = 5000;
export const MAX_BIKE = 120;
export const MAX_VERSION = 40;
export const MAX_NOTES = 2000;

/** The size limit for a mod's picture (`control-plane/src/mirror.ts` `MAX_THUMB_BYTES`). */
export const MAX_THUMB_BYTES = 2 * 1024 ** 2;
export const THUMB_EXTENSIONS = ["jpg", "jpeg", "png", "webp", "gif", "avif"];

/** "2026 RedBull KTM — Factory!" → "2026-redbull-ktm-factory" (`control-plane/src/modids.ts` `modSlug`). */
export function modSlug(title: string): string {
  const s = title
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/, "");
  return s || "mod";
}

/** Where a mod's page is: mxbsecure.com/mods/<slug>-<public id> (mxbsecure-web `pages/mods.tsx`). */
export function modPageUrl(title: string, assetId: string): string {
  return `https://mxbsecure.com/mods/${modSlug(title)}-${assetId}`;
}

/** Types whose files are made for one bike, so the bike is asked for. */
export function needsBike(type: ModKind | ""): boolean {
  return type === "paints" || type === "liveries";
}

export function limitFor(kind: string | null): number {
  return kind === "pnt" ? MAX_PNT_BYTES : MAX_UPLOAD_BYTES;
}

/** Why an upload can't start, before the form matters. */
export type AccountProblem =
  | { code: "signin" }
  | { code: "steam" }
  | { code: "blocked"; message: string }
  | { code: "openLimit"; limit: number };

export interface AccountState {
  /** `my_mods` failed with `signin`: no account token, or the control plane doesn't know it. */
  signedIn: boolean;
  /** Steam-confirmed (`uploads.ts:223`). */
  steam: boolean;
  /** The estate gate's refusal message, when there was one. */
  blocked: string | null;
  mine: MyMods | null;
  /** Uploads this app is still sending. */
  jobs: UploadJob[];
}

/** The error string a Rust command returned, as an account state. */
export function accountError(err: unknown): { signedIn: boolean; blocked: string | null } | null {
  const s = String(err);
  if (s === "signin") return { signedIn: false, blocked: null };
  if (s.startsWith("blocked:")) return { signedIn: true, blocked: s.slice("blocked:".length) };
  return null;
}

/** Uploads holding one of the open sessions (`uploads.ts:215`). */
export function openUploads(state: AccountState): number {
  const server = new Set((state.mine?.uploads ?? []).filter((u) => u.state === "open").map((u) => u.id));
  for (const j of state.jobs) {
    if (j.phase === "hashing") server.add(j.key);
  }
  return server.size;
}

export function accountProblem(state: AccountState): AccountProblem | null {
  if (state.blocked) return { code: "blocked", message: state.blocked };
  if (!state.signedIn) return { code: "signin" };
  if (!state.steam) return { code: "steam" };
  const limit = state.mine?.quota.openSessions ?? 3;
  if (openUploads(state) >= limit) return { code: "openLimit", limit };
  return null;
}

export type FileProblem =
  | { code: "noFile" }
  | { code: "fileType" }
  | { code: "fileEmpty" }
  | { code: "fileTooBig"; max: number };

export function fileProblem(file: PickedFile | null): FileProblem | null {
  if (!file) return { code: "noFile" };
  if (!file.kind) return { code: "fileType" };
  if (file.size <= 0) return { code: "fileEmpty" };
  const max = limitFor(file.kind);
  if (file.size > max) return { code: "fileTooBig", max };
  return null;
}

export type FieldProblem =
  | { field: "type"; code: "type" | "pntType" }
  | { field: "title"; code: "title" | "titleLong" }
  | { field: "bike"; code: "bike" | "bikeLong" }
  | { field: "description"; code: "descriptionLong" }
  | { field: "version"; code: "versionLong" }
  | { field: "notes"; code: "notesLong" };

export interface UploadForm {
  type: ModKind | "";
  title: string;
  bike: string;
  description: string;
  visibility: UploadMeta["visibility"];
  version: string;
  notes: string;
  /** A new version of this mod (its public id); null for a new one. */
  assetId: string | null;
  /** A picture to set once the mod is up; null for none. */
  thumbPath: string | null;
}

export const EMPTY_FORM: UploadForm = {
  type: "",
  title: "",
  bike: "",
  description: "",
  visibility: "public",
  version: "",
  notes: "",
  assetId: null,
  thumbPath: null,
};

/** Every field problem, in form order. A new version may leave title and type to the mod. */
export function fieldProblems(form: UploadForm, file: PickedFile | null): FieldProblem[] {
  const out: FieldProblem[] = [];
  const fresh = form.assetId === null;
  if (fresh && !form.type) out.push({ field: "type", code: "type" });
  else if (fresh && file?.kind === "pnt" && form.type !== "paints") out.push({ field: "type", code: "pntType" });
  const title = form.title.trim();
  if (fresh && !title) out.push({ field: "title", code: "title" });
  else if (title.length > MAX_TITLE) out.push({ field: "title", code: "titleLong" });
  if (fresh && needsBike(form.type) && !form.bike.trim()) out.push({ field: "bike", code: "bike" });
  else if (form.bike.length > MAX_BIKE) out.push({ field: "bike", code: "bikeLong" });
  if (form.description.length > MAX_DESCRIPTION) out.push({ field: "description", code: "descriptionLong" });
  if (form.version.trim().length > MAX_VERSION) out.push({ field: "version", code: "versionLong" });
  if (form.notes.length > MAX_NOTES) out.push({ field: "notes", code: "notesLong" });
  return out;
}

/** The form as the control plane wants it. */
export function toMeta(form: UploadForm): UploadMeta {
  return {
    title: form.title.trim(),
    type: (form.type || "other") as ModKind,
    description: form.description.trim(),
    bike: needsBike(form.type) || form.bike.trim() ? form.bike.trim() : "",
    visibility: form.visibility,
    version: form.version.trim() || null,
    notes: form.assetId !== null ? form.notes.trim() || null : null,
    assetId: form.assetId,
    thumbPath: form.thumbPath,
  };
}

/** Fraction done for a progress bar. */
export function progressOf(job: UploadJob): number {
  if (job.phase === "live" || job.phase === "checking" || job.phase === "completing") return 1;
  return job.size > 0 ? Math.min(1, job.sent / job.size) : 0;
}
