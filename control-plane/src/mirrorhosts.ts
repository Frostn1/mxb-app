/**
 * Turning a download link off a mod page into bytes: the mirror's port of the app's resolver
 * (`apps/manager/src-tauri/src/install.rs` `resolve_share` / `open_body`), host by host.
 *
 * A link is either a file, or a folder whose files are listed so each can be mirrored on its
 * own. Folder shapes the catalogue uses, and how each is expanded:
 *
 *  - MediaFire `…/folder/<key>`: the public API's `folder/get_content`, sub-folders and all.
 *  - Google Drive `…/drive/folders/<id>`: the folder page's embedded listing, recursively.
 *  - MEGA `…/folder/<h>#<key>` and `#F!h!k`: the folder API, node keys unwrapped with the
 *    folder key. Each file becomes `…/folder/<h>#<key>/file/<node>`, MEGA's own link form.
 *  - Dropbox `/sh/…` and `/scl/fo/…`: `dl=1` hands back the whole folder as one zip.
 *  - OneDrive / SharePoint folders: the shares API's `children`.
 *  - Pixeldrain lists `…/l/<id>`: `api/list/<id>`.
 *
 * Proton Drive is end-to-end encrypted with a key only its own client can use; it is refused.
 */

export const BROWSER_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36";

/** How far a folder share is walked, and how many files it may hold — the app's limits. */
export const FOLDER_MAX_DEPTH = 4;
export const FOLDER_MAX_FILES = 500;

export interface RemoteFile {
  /** Path inside the share, `/`-separated. */
  rel: string;
  url: string;
  size: number | null;
}

export type Resolved =
  | { kind: "file"; url: string }
  | { kind: "folder"; name: string; files: RemoteFile[] }
  /** MEGA: the bytes come encrypted, with the key to decrypt them. */
  | { kind: "mega"; url: string; size: number; name: string; key: Uint8Array; nonce: Uint8Array };

/** A refusal. `permanent` ones are not retried; `retryAfterMs` asks for a longer wait. `status`
 *  is the HTTP answer behind it, when there was one: a 401/403 hands the file to the fetcher. */
export class HostError extends Error {
  constructor(
    message: string,
    readonly permanent = false,
    readonly retryAfterMs?: number,
    readonly status?: number,
  ) {
    super(message);
  }
}

/** A link a Worker can't fetch but a fuller client (a headless browser) could: left for the runner. */
export class RunnerNeeded extends HostError {
  constructor(message: string) {
    super(message, true);
  }
}

type Fetch = typeof fetch;

/** Links mod pages carry that are never a download: donation, social and shop pages. */
const NOT_DOWNLOADS = [
  "paypal.me", "paypal.com", "discord.gg", "discord.com", "youtube.com", "youtu.be", "patreon.com", "ko-fi.com",
  "instagram.com", "tiktok.com", "twitter.com", "x.com", "facebook.com", "buymeacoffee.com", "myshopify.com",
  "l1nk.top", "linktr.ee",
];

export type Host =
  | "mediafire"
  | "gdrive"
  | "mega"
  | "dropbox"
  | "onedrive"
  | "pixeldrain"
  | "proton"
  | "shop"
  | "page"
  | "direct";

export function hostKind(url: string): Host {
  let h = "";
  try {
    h = new URL(url).hostname.toLowerCase();
  } catch {
    return "direct";
  }
  if (h.endsWith("mediafire.com")) return "mediafire";
  if (h.includes("drive.google") || h.includes("docs.google") || h.includes("drive.usercontent.google"))
    return "gdrive";
  if (h === "mega.nz" || h.endsWith(".mega.nz") || h === "mega.co.nz" || h.endsWith(".mega.co.nz") || h === "mega.io")
    return "mega";
  if (h.endsWith("dropbox.com") || h.endsWith("dropboxusercontent.com")) return "dropbox";
  if (h === "1drv.ms" || h.endsWith("onedrive.live.com") || h.endsWith("sharepoint.com")) return "onedrive";
  if (h.endsWith("pixeldrain.com")) return "pixeldrain";
  if (h.endsWith("proton.me")) return "proton";
  if (h.endsWith("mxbikes-shop.com")) return "shop";
  // Discord's CDN serves attachments; the rest of discord.com is pages.
  if (h === "cdn.discordapp.com" || h === "media.discordapp.net") return "direct";
  if (NOT_DOWNLOADS.some((d) => h === d || h.endsWith("." + d))) return "page";
  return "direct";
}

/** Resolve a share link. `allowFolder: false` for a file already listed out of a folder. */
export async function resolveShare(url: string, f: Fetch, allowFolder = true): Promise<Resolved> {
  const kind = hostKind(url);
  const folder = (r: Resolved): Resolved => {
    if (r.kind === "folder" && !allowFolder) throw new HostError("a folder inside a listed folder", true);
    return r;
  };
  switch (kind) {
    case "proton":
      throw new HostError("Proton Drive links are end-to-end encrypted and can't be fetched", true);
    case "shop":
      throw new HostError("a shop product page, not a download", true);
    case "page":
      throw new HostError("not a download link", true);
    case "mediafire": {
      const key = mediafireFolderKey(url);
      if (key) return folder(await mediafireFolder(f, key, url));
      return { kind: "file", url: await resolveMediafire(f, url) };
    }
    case "gdrive":
      if (isGdriveFolder(url)) return folder(await gdriveFolder(f, url));
      return { kind: "file", url: gdriveDirect(url) };
    case "mega":
      return folder(await resolveMega(f, url));
    case "dropbox":
      return { kind: "file", url: dropboxDirect(url) };
    case "onedrive":
      return folder(await resolveOneDrive(f, url));
    case "pixeldrain":
      return folder(await resolvePixeldrain(f, url));
    default:
      return { kind: "file", url };
  }
}

// ───────────────────────────── MediaFire ─────────────────────────────

function mediafireApi(path: string, query: string): string {
  return `https://www.mediafire.com/api/1.5/${path}?${query}&response_format=json`;
}

export function mediafireQuickKey(url: string): string | null {
  const byPath = /\/(?:file|file_premium|download|view)\/([a-z0-9]{11}(?:[a-z0-9]{4})?)/i.exec(url);
  const byQuery = /[?&]([a-z0-9]{11}(?:[a-z0-9]{4})?)(?:[&#]|$)/i.exec(url);
  return (byPath ?? byQuery)?.[1] ?? null;
}

export function mediafireFolderKey(url: string): string | null {
  return (/\/folder\/([a-z0-9]+)/i.exec(url) ?? /[?&]sharekey=([a-z0-9]+)/i.exec(url))?.[1] ?? null;
}

export function isMediafireDirect(url: string): boolean {
  return /^https?:\/\/download[0-9]*\.mediafire\.com\//i.test(url);
}

function usableLink(href: string): string | null {
  const h = decodeHtml(href.trim());
  if (h.startsWith("//")) return `https:${h}`;
  return /^https?:\/\//i.test(h) ? h : null;
}

function decodeHtml(s: string): string {
  return s
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

function decodeScrambled(v: string): string | null {
  try {
    return usableLink(atob(v.trim()));
  } catch {
    return null;
  }
}

/** The app's `parse_mediafire_link`: every place MediaFire has put the CDN link. */
export function parseMediafireLink(html: string): string | null {
  for (const m of html.matchAll(/data-scrambled-url\s*=\s*"([^"]*)"/gi)) {
    const u = decodeScrambled(m[1]);
    if (u) return u;
  }
  for (const m of html.matchAll(/scrambled[_-]?url["'\s:=]+([A-Za-z0-9+/=]{24,})/gi)) {
    const u = decodeScrambled(m[1]);
    if (u) return u;
  }
  for (const tag of html.match(/<a\b[^>]*>/gi) ?? []) {
    if (/\bid\s*=\s*"downloadButton"/i.test(tag) || /aria-label\s*=\s*['"]Download file['"]/i.test(tag)) {
      const href = /\bhref\s*=\s*"([^"]*)"/i.exec(tag)?.[1];
      const u = href ? usableLink(href) : null;
      if (u) return u;
    }
  }
  const flat = html.replace(/\\\//g, "/");
  const direct = /(?:https?:)?\/\/download[0-9]*\.mediafire\.com\/[^"'<>\\ ]+/i.exec(flat);
  return direct ? usableLink(direct[0]) : null;
}

/** MediaFire's refusals, from an API message or the page's own copy. */
export function mediafireRefusal(text: string): HostError | null {
  const t = text.toLowerCase();
  if (
    t.includes("invalid or deleted file") ||
    t.includes("has been removed") ||
    t.includes("has been deleted") ||
    t.includes("unknown or invalid quickkey") ||
    t.includes("file not found")
  )
    return new HostError("MediaFire: the file no longer exists", true);
  if (t.includes("enter password") || t.includes("password to access"))
    return new HostError("MediaFire: password-protected", true);
  if (t.includes("violation of our terms") || t.includes("dangerous file"))
    return new HostError("MediaFire: blocked by MediaFire", true);
  if (t.includes("bandwidth limit") || t.includes("daily download limit"))
    return new HostError("MediaFire: download limit reached", false, 6 * 3600_000);
  return null;
}

const PAGE_HEADERS = {
  "user-agent": BROWSER_UA,
  accept: "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
  "accept-language": "en-US,en;q=0.9",
};

async function getText(f: Fetch, url: string, headers: Record<string, string> = PAGE_HEADERS): Promise<string> {
  const res = await f(url, { headers });
  if (res.status === 404 || res.status === 410) throw new HostError(`${hostKind(url)}: ${res.status}`, true);
  if (!res.ok) throw new HostError(`${hostKind(url)}: answered ${res.status}`, false, undefined, res.status);
  return res.text();
}

async function getJson(f: Fetch, url: string, init?: RequestInit): Promise<any> {
  const res = await f(url, init ?? { headers: { "user-agent": BROWSER_UA, accept: "application/json" } });
  const text = await res.text();
  try {
    return JSON.parse(text);
  } catch {
    if (res.status === 404) throw new HostError(`${hostKind(url)}: not found`, true);
    throw new HostError(`${hostKind(url)}: answered ${res.status} without JSON`, false, undefined, res.status);
  }
}

/** Page first, API second — the order the app measured as the working one. */
async function resolveMediafire(f: Fetch, url: string): Promise<string> {
  if (isMediafireDirect(url)) return url;
  const html = await getText(f, url);
  const direct = parseMediafireLink(html);
  if (direct) return direct;
  const key = mediafireQuickKey(url);
  if (key) {
    const api = await getJson(f, mediafireApi("file/get_links.php", `quick_key=${key}&link_type=direct_download`)).catch(
      () => null,
    );
    const r = api?.response;
    if (r?.result === "Error") throw mediafireRefusal(String(r.message ?? "")) ?? new HostError(`MediaFire refused (${r.message})`);
    const link = r?.links?.[0]?.direct_download;
    const u = typeof link === "string" ? usableLink(link) : null;
    if (u && isMediafireDirect(u)) return u;
  }
  throw mediafireRefusal(html) ?? new HostError("MediaFire: no download link on the page");
}

async function mediafireContent(f: Fetch, folderKey: string, type: "files" | "folders"): Promise<any[]> {
  const out: any[] = [];
  for (let chunk = 1; chunk <= 10; chunk++) {
    const j = await getJson(
      f,
      mediafireApi(
        "folder/get_content.php",
        `folder_key=${folderKey}&content_type=${type}&chunk=${chunk}&chunk_size=100`,
      ),
    );
    const r = j?.response;
    if (!r) throw new HostError("MediaFire: unreadable folder listing");
    if (r.result === "Error") throw mediafireRefusal(String(r.message ?? "")) ?? new HostError(`MediaFire refused (${r.message})`, true);
    const items = r.folder_content?.[type];
    if (Array.isArray(items)) out.push(...items);
    if (r.folder_content?.more_chunks !== "yes") break;
  }
  return out;
}

async function mediafireWalk(f: Fetch, key: string, prefix: string, out: RemoteFile[], depth: number): Promise<void> {
  if (depth > FOLDER_MAX_DEPTH || out.length >= FOLDER_MAX_FILES) return;
  for (const file of await mediafireContent(f, key, "files")) {
    if (out.length >= FOLDER_MAX_FILES) break;
    const name = typeof file?.filename === "string" ? file.filename.trim() : "";
    const link = file?.links?.normal_download;
    if (!usableName(name) || typeof link !== "string") continue;
    const size = Number(file.size);
    out.push({ rel: prefix + name, url: link, size: Number.isFinite(size) ? size : null });
  }
  for (const d of await mediafireContent(f, key, "folders")) {
    const name = typeof d?.name === "string" ? d.name.trim() : "";
    if (typeof d?.folderkey !== "string" || !usableName(name)) continue;
    await mediafireWalk(f, d.folderkey, `${prefix}${sanitize(name)}/`, out, depth + 1);
  }
}

async function mediafireFolder(f: Fetch, key: string, url: string): Promise<Resolved> {
  const files: RemoteFile[] = [];
  await mediafireWalk(f, key, "", files, 0);
  if (files.length === 0) throw new HostError("MediaFire: the folder is empty", true);
  const name = /\/folder\/[a-z0-9]+\/([^/?#]+)/i.exec(url)?.[1];
  return { kind: "folder", name: name ? safeDecode(name).replace(/[_+]/g, " ") : "mod", files };
}

// ───────────────────────────── Google Drive ─────────────────────────────

export function gdriveId(url: string): string | null {
  return (/\/d\/([A-Za-z0-9_-]+)/.exec(url) ?? /[?&]id=([A-Za-z0-9_-]+)/.exec(url))?.[1] ?? null;
}

/** usercontent serves the bytes; a large file still answers with the virus-scan form. */
export function gdriveDirect(url: string): string {
  const id = gdriveId(url);
  return id ? `https://drive.usercontent.google.com/download?id=${id}&export=download` : url;
}

export function isGdriveFolder(url: string): boolean {
  const u = url.toLowerCase();
  return u.includes("/folders/") || u.includes("/folderview");
}

function gdriveFolderId(url: string): string | null {
  return (/\/folders\/([A-Za-z0-9_-]+)/.exec(url) ?? /[?&]id=([A-Za-z0-9_-]+)/.exec(url))?.[1] ?? null;
}

const GDRIVE_FOLDER_MIME = "application/vnd.google-apps.folder";

/** The app's `parse_gdrive_folder`: `[id,[parent],name,mime]` tuples in the page's data blob. */
export function parseGdriveFolder(html: string, folderId: string): { id: string; name: string; mime: string }[] {
  const text = html
    .replace(/\\x5b/g, "[")
    .replace(/\\x5d/g, "]")
    .replace(/\\x22/g, '"')
    .replace(/\\\//g, "/");
  const esc = folderId.replace(/[.*+?^${}()|[\]\\-]/g, "\\$&");
  const re = new RegExp(`"([A-Za-z0-9_-]{20,})",\\["${esc}"\\],"((?:[^"\\\\]|\\\\.)*?)","([^"]+)"`, "g");
  const seen = new Set<string>();
  const out: { id: string; name: string; mime: string }[] = [];
  for (const m of text.matchAll(re)) {
    if (seen.has(m[1])) continue;
    seen.add(m[1]);
    out.push({ id: m[1], name: m[2], mime: m[3] });
  }
  return out;
}

function pageTitle(html: string): string | null {
  const t = /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html)?.[1]?.trim();
  return t ? decodeHtml(t) : null;
}

async function gdriveWalk(f: Fetch, id: string, prefix: string, out: RemoteFile[], depth: number): Promise<string> {
  if (depth > FOLDER_MAX_DEPTH || out.length >= FOLDER_MAX_FILES) return "";
  const html = await getText(f, `https://drive.google.com/drive/folders/${id}`);
  for (const e of parseGdriveFolder(html, id)) {
    if (e.mime === GDRIVE_FOLDER_MIME) {
      await gdriveWalk(f, e.id, `${prefix}${sanitize(e.name)}/`, out, depth + 1);
      continue;
    }
    if (out.length >= FOLDER_MAX_FILES) break;
    if (!usableName(e.name)) continue;
    out.push({ rel: prefix + e.name, url: `https://drive.google.com/file/d/${e.id}/view`, size: null });
  }
  return (pageTitle(html) ?? "").replace(/ - Google Drive$/, "").trim();
}

async function gdriveFolder(f: Fetch, url: string): Promise<Resolved> {
  const id = gdriveFolderId(url);
  if (!id) throw new HostError("Google Drive: no folder id in the link", true);
  const files: RemoteFile[] = [];
  const name = await gdriveWalk(f, id, "", files, 0);
  if (files.length === 0) {
    if (name.includes("Sign-in")) throw new HostError("Google Drive: the folder isn't shared publicly", true);
    throw new HostError("Google Drive: the folder has no downloadable file", true);
  }
  return { kind: "folder", name: name || "mod", files };
}

/** The virus-scan form a large file answers with: its action and fields. */
export function parseGdriveConfirm(html: string): string | null {
  const form = /<form\b[^>]*>([\s\S]*?)<\/form>/i.exec(html);
  if (!form) return null;
  const action = /\baction\s*=\s*"([^"]+)"/i.exec(form[0])?.[1];
  if (!action) return null;
  const u = new URL(decodeHtml(action), "https://drive.usercontent.google.com/");
  let any = false;
  for (const input of form[1].match(/<input\b[^>]*>/gi) ?? []) {
    const name = /\bname\s*=\s*"([^"]+)"/i.exec(input)?.[1];
    if (!name) continue;
    u.searchParams.set(name, decodeHtml(/\bvalue\s*=\s*"([^"]*)"/i.exec(input)?.[1] ?? ""));
    any = true;
  }
  return any ? u.toString() : null;
}

export function gdrivePageError(html: string): HostError | null {
  const t = (pageTitle(html) ?? "").toLowerCase();
  if (t.includes("quota")) return new HostError("Google Drive: download quota exceeded", false, 12 * 3600_000);
  if (t.includes("access denied") || t.includes("permission")) return new HostError("Google Drive: not shared publicly", true);
  if (t.includes("not found")) return new HostError("Google Drive: the file no longer exists", true);
  return null;
}

// ───────────────────────────── Dropbox ─────────────────────────────

/** `dl=1` turns a file share into the file, and a folder share into a zip of the folder. */
export function dropboxDirect(url: string): string {
  try {
    const u = new URL(url);
    if (u.hostname.endsWith("dropboxusercontent.com")) return url;
    u.searchParams.delete("raw");
    u.searchParams.set("dl", "1");
    return u.toString();
  } catch {
    return url;
  }
}

// ───────────────────────────── OneDrive / SharePoint ─────────────────────────────

/** The shares API's id for a sharing link: `u!` + unpadded base64url of the URL. */
export function oneDriveShareId(url: string): string {
  const b64 = btoa(String.fromCharCode(...new TextEncoder().encode(url)));
  return "u!" + b64.replace(/=+$/, "").replace(/\//g, "_").replace(/\+/g, "-");
}

async function resolveOneDrive(f: Fetch, url: string): Promise<Resolved> {
  const host = new URL(url).hostname;
  if (host.endsWith("sharepoint.com")) {
    // A SharePoint file link downloads with `download=1`; a folder (`/:f:/`) has no public
    // listing without a Graph token, so it is left to the runner.
    if (/\/:f:\//.test(url)) throw new RunnerNeeded("SharePoint folder links need a browser");
    const u = new URL(url);
    u.searchParams.set("download", "1");
    return { kind: "file", url: u.toString() };
  }
  const base = `https://api.onedrive.com/v1.0/shares/${oneDriveShareId(url)}`;
  const item = await getJson(f, `${base}/root?$expand=children`).catch(() => null);
  // Personal OneDrive links moved to SharePoint in 2025 and no longer answer the anonymous
  // shares API; they only open in a browser. Left for the runner rather than failed.
  if (!item || item.error) throw new RunnerNeeded(`OneDrive: ${item?.error?.code ?? "needs a browser"}`);
  if (item?.folder) {
    const files: RemoteFile[] = [];
    for (const c of Array.isArray(item.children) ? item.children : []) {
      if (c?.file && typeof c["@content.downloadUrl"] === "string" && usableName(c.name)) {
        files.push({ rel: c.name, url: c["@content.downloadUrl"], size: Number(c.size) || null });
      }
      if (files.length >= FOLDER_MAX_FILES) break;
    }
    if (files.length === 0) throw new HostError("OneDrive: the folder is empty", true);
    return { kind: "folder", name: String(item.name ?? "mod"), files };
  }
  return { kind: "file", url: typeof item?.["@content.downloadUrl"] === "string" ? item["@content.downloadUrl"] : `${base}/root/content` };
}

// ───────────────────────────── Pixeldrain ─────────────────────────────

async function resolvePixeldrain(f: Fetch, url: string): Promise<Resolved> {
  const file = /\/(?:u|api\/file)\/([A-Za-z0-9]+)/.exec(url);
  if (file) return { kind: "file", url: `https://pixeldrain.com/api/file/${file[1]}?download` };
  const list = /\/l\/([A-Za-z0-9]+)/.exec(url);
  if (!list) return { kind: "file", url };
  const j = await getJson(f, `https://pixeldrain.com/api/list/${list[1]}`);
  const files: RemoteFile[] = (Array.isArray(j?.files) ? j.files : [])
    .filter((x: any) => typeof x?.id === "string" && usableName(x?.name))
    .slice(0, FOLDER_MAX_FILES)
    .map((x: any) => ({ rel: x.name, url: `https://pixeldrain.com/api/file/${x.id}?download`, size: Number(x.size) || null }));
  if (files.length === 0) throw new HostError("Pixeldrain: the list is empty", true);
  return { kind: "folder", name: String(j?.title ?? "mod"), files };
}

// ───────────────────────────── MEGA ─────────────────────────────

/**
 * MEGA links, in all three shapes the catalogue has:
 * `…/file/<h>#<key>`, `…/folder/<h>#<key>[/file/<node>]` and the legacy `#!h!k` / `#F!h!k`.
 */
export function parseMegaLink(
  url: string,
): { kind: "file"; handle: string; key: string } | { kind: "folder"; handle: string; key: string; node: string | null } | null {
  const file = /\/file\/([A-Za-z0-9_-]+)#([A-Za-z0-9_-]+)/.exec(url);
  const folder = /\/folder\/([A-Za-z0-9_-]+)#([A-Za-z0-9_-]+)(?:\/(?:file|folder)\/([A-Za-z0-9_-]+))?/.exec(url);
  if (folder) return { kind: "folder", handle: folder[1], key: folder[2], node: folder[3] ?? null };
  if (file) return { kind: "file", handle: file[1], key: file[2] };
  const legacyFolder = /#F!([A-Za-z0-9_-]+)!([A-Za-z0-9_-]+)(?:!([A-Za-z0-9_-]+))?/.exec(url);
  if (legacyFolder) return { kind: "folder", handle: legacyFolder[1], key: legacyFolder[2], node: legacyFolder[3] ?? null };
  const legacyFile = /#!([A-Za-z0-9_-]+)!([A-Za-z0-9_-]+)/.exec(url);
  if (legacyFile) return { kind: "file", handle: legacyFile[1], key: legacyFile[2] };
  return null;
}

export function b64urlDecode(s: string): Uint8Array {
  const b = s.replace(/-/g, "+").replace(/_/g, "/").replace(/,/g, "");
  const bin = atob(b + "=".repeat((4 - (b.length % 4)) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

/** A 32-byte file key folds to the AES key (halves XORed) and the CTR nonce (bytes 16..24). */
export function megaFileKey(raw: Uint8Array): { key: Uint8Array; nonce: Uint8Array } {
  if (raw.length !== 32) throw new HostError("MEGA: malformed file key", true);
  const key = new Uint8Array(16);
  for (let i = 0; i < 16; i++) key[i] = raw[i] ^ raw[i + 16];
  return { key, nonce: raw.slice(16, 24) };
}

/** AES-ECB decryption of whole blocks, made from the CBC primitive WebCrypto does have. */
export async function aesEcbDecrypt(key: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
  const k = await crypto.subtle.importKey("raw", key, "AES-CBC", false, ["encrypt", "decrypt"]);
  const out = new Uint8Array(data.length);
  const zero = new Uint8Array(16);
  for (let i = 0; i < data.length; i += 16) {
    const block = data.slice(i, i + 16);
    // CBC decrypt insists on PKCS#7, so a valid padding block is appended: E(pad ^ block).
    const padBlock = new Uint8Array(
      await crypto.subtle.encrypt({ name: "AES-CBC", iv: block }, k, new Uint8Array(16).fill(16)),
    ).slice(0, 16);
    const plain = new Uint8Array(
      await crypto.subtle.decrypt({ name: "AES-CBC", iv: zero }, k, concat(block, padBlock)),
    );
    out.set(plain.slice(0, 16), i);
  }
  return out;
}

/** MEGA's attribute blob: AES-CBC with a zero IV and no padding, `MEGA{json}` inside. */
async function megaAttrs(key: Uint8Array, at: string): Promise<{ n?: string }> {
  const data = b64urlDecode(at);
  const k = await crypto.subtle.importKey("raw", key, "AES-CBC", false, ["encrypt", "decrypt"]);
  const last = data.slice(data.length - 16);
  const padBlock = new Uint8Array(
    await crypto.subtle.encrypt({ name: "AES-CBC", iv: last }, k, new Uint8Array(16).fill(16)),
  ).slice(0, 16);
  const plain = new Uint8Array(
    await crypto.subtle.decrypt({ name: "AES-CBC", iv: new Uint8Array(16) }, k, concat(data, padBlock)),
  );
  const text = new TextDecoder().decode(plain).replace(/\0+$/, "");
  if (!text.startsWith("MEGA")) throw new HostError("MEGA: wrong key", true);
  try {
    return JSON.parse(text.slice(4));
  } catch {
    return {};
  }
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
}

let megaSeq = Math.floor(Math.random() * 1e9);

async function megaApi(f: Fetch, body: unknown, folder?: string): Promise<any> {
  const u = new URL("https://g.api.mega.co.nz/cs");
  u.searchParams.set("id", String(megaSeq++));
  if (folder) u.searchParams.set("n", folder);
  const res = await f(u.toString(), {
    method: "POST",
    headers: { "content-type": "application/json", "user-agent": BROWSER_UA },
    body: JSON.stringify([body]),
  });
  if (!res.ok) throw new HostError(`MEGA: API answered ${res.status}`);
  const j = (await res.json()) as unknown;
  const r = Array.isArray(j) ? j[0] : j;
  if (typeof r === "number") {
    // -3 EAGAIN, -4 rate limited, -17 over quota: transient. -9 ENOENT, -11 EACCESS,
    // -16 blocked/taken down: permanent.
    if (r === -3 || r === -4 || r === -17 || r === -18) throw new HostError(`MEGA: busy (${r})`, false, 3600_000);
    throw new HostError(`MEGA: refused (${r})`, r === -9 || r === -11 || r === -16 || r === -2);
  }
  return r;
}

async function resolveMega(f: Fetch, url: string): Promise<Resolved> {
  const link = parseMegaLink(url);
  if (!link) throw new HostError("MEGA: unrecognised link", true);
  if (link.kind === "file") {
    const { key, nonce } = megaFileKey(b64urlDecode(link.key));
    const r = await megaApi(f, { a: "g", g: 1, ssl: 2, p: link.handle });
    if (typeof r?.g !== "string") throw new HostError("MEGA: no download URL", false);
    const attrs = await megaAttrs(key, String(r.at ?? ""));
    return { kind: "mega", url: r.g, size: Number(r.s), name: attrs.n ?? link.handle, key, nonce };
  }

  // A folder: every node, keys unwrapped with the folder's key.
  const folderKey = b64urlDecode(link.key);
  if (folderKey.length !== 16) throw new HostError("MEGA: malformed folder key", true);
  const listing = await megaApi(f, { a: "f", c: 1, r: 1, ca: 1 }, link.handle);
  const nodes: any[] = Array.isArray(listing?.f) ? listing.f : [];
  const byHandle = new Map<string, any>(nodes.map((n) => [n.h, n]));
  const nodeKey = async (n: any): Promise<Uint8Array | null> => {
    const wrapped = typeof n?.k === "string" ? n.k.split("/")[0].split(":").pop() : null;
    if (!wrapped) return null;
    return aesEcbDecrypt(folderKey, b64urlDecode(wrapped));
  };

  if (link.node) {
    // One file inside the folder, as listed out by an earlier expansion.
    const n = byHandle.get(link.node);
    if (!n || n.t !== 0) throw new HostError("MEGA: the file is no longer in the folder", true);
    const raw = await nodeKey(n);
    if (!raw || raw.length !== 32) throw new HostError("MEGA: unreadable node key", true);
    const { key, nonce } = megaFileKey(raw);
    const r = await megaApi(f, { a: "g", g: 1, ssl: 2, n: n.h }, link.handle);
    if (typeof r?.g !== "string") throw new HostError("MEGA: no download URL", false);
    const attrs = await megaAttrs(key, String(n.a ?? r.at ?? ""));
    return { kind: "mega", url: r.g, size: Number(r.s ?? n.s), name: attrs.n ?? n.h, key, nonce };
  }

  // Paths from each file node up to the root.
  const names = new Map<string, string>();
  for (const n of nodes) {
    const raw = await nodeKey(n).catch(() => null);
    if (!raw) continue;
    const k = raw.length === 32 ? megaFileKey(raw).key : raw;
    const attrs = await megaAttrs(k, String(n.a ?? "")).catch(() => ({}) as { n?: string });
    if (attrs.n) names.set(n.h, attrs.n);
  }
  const root = nodes.find((n) => !byHandle.has(n.p));
  const pathOf = (n: any): string => {
    const parts: string[] = [];
    for (let cur = byHandle.get(n.p), i = 0; cur && cur !== root && i < FOLDER_MAX_DEPTH + 2; cur = byHandle.get(cur.p), i++) {
      parts.unshift(sanitize(names.get(cur.h) ?? cur.h));
    }
    return parts.length ? parts.join("/") + "/" : "";
  };
  const files: RemoteFile[] = nodes
    .filter((n) => n.t === 0 && names.has(n.h))
    .slice(0, FOLDER_MAX_FILES)
    .map((n) => ({
      rel: pathOf(n) + names.get(n.h)!,
      url: `https://mega.nz/folder/${link.handle}#${link.key}/file/${n.h}`,
      size: Number(n.s) || null,
    }));
  if (files.length === 0) throw new HostError("MEGA: the folder is empty", true);
  return { kind: "folder", name: (root && names.get(root.h)) || "mod", files };
}

/**
 * Decrypt a MEGA download as it streams: AES-128-CTR, the counter being the nonce followed by
 * the 64-bit block index. CTR is seekable, so each chunk is decrypted on its own at its offset
 * (chunks are cut on 16-byte boundaries, with any remainder carried to the next).
 */
export function megaDecryptStream(key: Uint8Array, nonce: Uint8Array): TransformStream<Uint8Array, Uint8Array> {
  let offset = 0;
  let carry = new Uint8Array(0);
  let cryptoKey: CryptoKey | null = null;
  const run = async (data: Uint8Array): Promise<Uint8Array> => {
    cryptoKey ??= await crypto.subtle.importKey("raw", key, "AES-CTR", false, ["decrypt"]);
    const counter = new Uint8Array(16);
    counter.set(nonce, 0);
    let block = BigInt(offset / 16);
    for (let i = 15; i >= 8; i--) {
      counter[i] = Number(block & 0xffn);
      block >>= 8n;
    }
    offset += data.length;
    return new Uint8Array(await crypto.subtle.decrypt({ name: "AES-CTR", counter, length: 64 }, cryptoKey, data));
  };
  return new TransformStream({
    async transform(chunk, ctl) {
      const all = carry.length ? concat(carry, chunk) : chunk;
      const whole = all.length - (all.length % 16);
      carry = all.slice(whole);
      if (whole > 0) ctl.enqueue(await run(all.subarray(0, whole)));
    },
    async flush(ctl) {
      if (carry.length) ctl.enqueue(await run(carry));
    },
  });
}

// ───────────────────────────── opening a body ─────────────────────────────

/**
 * GET a resolved file URL and hand back a response that is really the file — the app's
 * `open_body`. Drive's virus-scan form is submitted; every host's 200-with-a-web-page refusal
 * is read for what it says.
 */
export async function openBody(f: Fetch, url: string): Promise<Response> {
  let res = await f(url, { headers: { "user-agent": BROWSER_UA }, redirect: "follow" });
  const isGdrive = url.includes("google");
  if (res.status === 429 || res.status >= 500) {
    const after = Number(res.headers.get("retry-after"));
    throw new HostError(`${hostKind(url)}: answered ${res.status}`, false, Number.isFinite(after) && after > 0 ? after * 1000 : undefined);
  }
  if (res.status === 404 || res.status === 410) throw new HostError(`${hostKind(url)}: the file is gone (${res.status})`, true);
  if (res.status === 401 || res.status === 403)
    throw new HostError(`${hostKind(url)}: refused (${res.status})`, isGdrive ? false : true, undefined, res.status);
  if (!res.ok) throw new HostError(`${hostKind(url)}: answered ${res.status}`);

  if (isHtml(res) && isGdrive) {
    const html = await res.text();
    const confirm = parseGdriveConfirm(html);
    if (!confirm) throw gdrivePageError(html) ?? new HostError("Google Drive: unexpected page");
    res = await f(confirm, { headers: { "user-agent": BROWSER_UA } });
    if (res.status === 429 || res.status >= 500) throw new HostError(`Google Drive: answered ${res.status}`);
    if (!res.ok) throw new HostError(`Google Drive: answered ${res.status}`);
  }
  if (isHtml(res)) {
    const html = await res.text();
    if (isGdrive) throw gdrivePageError(html) ?? new HostError("Google Drive: a web page instead of the file");
    if (url.includes("mediafire")) throw mediafireRefusal(html) ?? new HostError("MediaFire: a web page instead of the file");
    throw new HostError(`${hostKind(url)}: a web page instead of a file`, true);
  }
  return res;
}

function isHtml(res: Response): boolean {
  return (res.headers.get("content-type") ?? "").toLowerCase().startsWith("text/html");
}

/** The file's own name: Content-Disposition, else the URL. */
export function filenameFrom(res: Response, url: string): string {
  const cd = res.headers.get("content-disposition") ?? "";
  const star = /filename\*\s*=\s*(?:UTF-8'')?"?([^";]+)"?/i.exec(cd);
  const plain = /filename\s*=\s*"?([^";]+)"?/i.exec(cd);
  const raw = star ? safeDecode(star[1]) : plain?.[1];
  if (raw && usableName(raw)) return sanitize(raw.trim());
  const path = (() => {
    try {
      return new URL(res.url || url).pathname;
    } catch {
      return "";
    }
  })();
  const last = safeDecode(path.split("/").filter(Boolean).pop() ?? "");
  return usableName(last) && /\.[a-z0-9]{2,5}$/i.test(last) ? sanitize(last) : "download.bin";
}

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

function usableName(name: unknown): name is string {
  return typeof name === "string" && name.trim() !== "" && name.trim() !== "." && name.trim() !== "..";
}

export function sanitize(name: string): string {
  return name.replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").slice(0, 200);
}
