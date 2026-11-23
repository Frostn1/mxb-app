/**
 * The app's end of the `postMessage` bridge: the only door out of an extension's frame.
 *
 * Every message is checked in this order, and the first failure answers with an error:
 *   1. it came from *this* frame's window (`event.source`), not another frame or the app;
 *   2. it is shaped like a call (`mxbext: 1`, numeric id, known method);
 *   3. the extension is still installed and enabled;
 *   4. the method's permission (`METHOD_PERMISSION`) is in the extension's *granted* list,
 *      read fresh on every call so a revoke in the Extensions page takes effect at once;
 *   5. it is inside the rate limit.
 *
 * Nothing here calls `invoke` on the frame's behalf with frame-chosen arguments. Each method
 * is a fixed function of the app's own, and its answer goes through `data.ts`, which builds
 * the reply field by field.
 */

import { cleanLabel, type InstalledExtension, type ExtSetting } from "./manifest";
import { METHOD_PERMISSION } from "./permissions";
import { extensionData, STUDIO_SCREENS, type StudioScreen } from "./data";
import { getSnapshot } from "./store";

export type ToastKind = "info" | "success" | "error";

export interface BridgeHost {
  toast(text: string, kind: ToastKind, ext: InstalledExtension): void;
  /** Ask the person before opening a link an extension handed over. */
  confirmOpen(url: string, ext: InstalledExtension): Promise<boolean>;
  openUrl(url: string): Promise<void>;
  /** The app's current theme, as a view gets it. */
  theme(): { css: string; mode: "light" | "dark" };
  locale(): string;
}

interface Handle {
  ext: string;
  main: boolean;
  emit(event: string, data: unknown): void;
}

const frames = new Map<string, Set<Handle>>();
const logs = new Map<string, { at: number; level: string; text: string }[]>();
const LOG_LINES = 300;

/** The last console lines and errors from an extension's frames (for extension_logs). */
export function extensionLogs(id: string): { at: number; level: string; text: string }[] {
  return [...(logs.get(id) ?? [])];
}
const toolCalls = new Map<string, { ext: string; resolve: (v: unknown) => void; reject: (e: Error) => void }>();

const STORAGE_CAP = 512 * 1024;
const RATE_PER_SEC = 60;

function current(id: string): InstalledExtension | undefined {
  return getSnapshot().installed.find((e) => e.id === id && e.enabled && !e.suspended);
}

function kvKey(id: string, kind: "kv" | "settings") {
  return `mxbext:${id}:${kind}`;
}

function readJson(key: string): Record<string, unknown> {
  try {
    const v = JSON.parse(localStorage.getItem(key) ?? "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

/** Settings with the manifest's defaults filled in, and only keys the manifest declares. */
export function extensionSettings(ext: InstalledExtension): Record<string, unknown> {
  const decl: ExtSetting[] = Array.isArray(ext.manifest.contributes?.settings) ? ext.manifest.contributes!.settings! : [];
  const stored = readJson(kvKey(ext.id, "settings"));
  const out: Record<string, unknown> = {};
  for (const s of decl) {
    const v = stored[s.key];
    const ok =
      (s.type === "boolean" && typeof v === "boolean") ||
      (s.type === "number" && typeof v === "number" && Number.isFinite(v)) ||
      (s.type === "string" && typeof v === "string") ||
      (s.type === "enum" && typeof v === "string" && (s.options ?? []).includes(v));
    out[s.key] = ok ? v : (s.default ?? null);
  }
  return out;
}

export function setExtensionSetting(ext: InstalledExtension, key: string, value: unknown): void {
  const k = kvKey(ext.id, "settings");
  const stored = readJson(k);
  stored[key] = value;
  localStorage.setItem(k, JSON.stringify(stored));
  emitTo(ext.id, "settings.changed", extensionSettings(ext));
}

/** Drop everything an extension stored. Called on uninstall. */
export function clearExtensionData(id: string): void {
  localStorage.removeItem(kvKey(id, "kv"));
  localStorage.removeItem(kvKey(id, "settings"));
}

/** Send an event to every frame of one extension. */
export function emitTo(ext: string, event: string, data: unknown): void {
  for (const h of frames.get(ext) ?? []) h.emit(event, data);
}

/** Send an event to every frame of every extension (theme changes). */
export function emitAll(event: string, data: unknown): void {
  for (const set of frames.values()) for (const h of set) h.emit(event, data);
}

/** Run one of an extension's commands: delivered to its frames, which act on it. */
export function runCommand(ext: string, command: string): boolean {
  const set = frames.get(ext);
  if (!set?.size) return false;
  for (const h of set) h.emit("command", { id: command });
  return true;
}

/**
 * Call an MCP tool an extension declared and handles. The hook an MCP server inside the app
 * would use; see docs/extensions.md for how tools are exposed to an outside client.
 */
export function callExtensionTool(ext: string, name: string, args: unknown, timeoutMs = 10_000): Promise<unknown> {
  const e = current(ext);
  if (!e) return Promise.reject(new Error("that extension isn't enabled"));
  if (!e.granted.includes("mcp.tools")) return Promise.reject(new Error("mcp.tools isn't granted"));
  const tools = e.manifest.contributes?.mcp?.tools ?? [];
  if (!Array.isArray(tools) || !tools.some((t) => t?.name === name)) {
    return Promise.reject(new Error(`${ext} declares no tool '${name}'`));
  }
  const set = [...(frames.get(ext) ?? [])];
  const target = set.find((h) => h.main) ?? set[0];
  if (!target) return Promise.reject(new Error(`${e.manifest.name} isn't running`));
  const callId = crypto.randomUUID();
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      toolCalls.delete(callId);
      reject(new Error(`${name} timed out`));
    }, timeoutMs);
    toolCalls.set(callId, {
      ext,
      resolve: (v) => {
        window.clearTimeout(timer);
        resolve(v);
      },
      reject: (err) => {
        window.clearTimeout(timer);
        reject(err);
      },
    });
    target.emit("mcp.call", { callId, name, args });
  });
}

/** Every tool from every enabled extension that may expose them. */
export function extensionTools(): { ext: string; name: string; description: string; inputSchema?: unknown }[] {
  return getSnapshot()
    .installed.filter((e) => e.enabled && !e.suspended && e.granted.includes("mcp.tools"))
    .flatMap((e) =>
      (e.manifest.contributes?.mcp?.tools ?? [])
        .filter((t) => typeof t?.name === "string" && /^[a-z0-9_]{1,64}$/.test(t.name))
        .map((t) => ({ ext: e.id, name: t.name, description: cleanLabel(t.description, 300), inputSchema: t.inputSchema })),
    );
}

type Method = (ext: InstalledExtension, params: Record<string, unknown>, h: FrameState) => unknown;

interface FrameState {
  gameTimer: number | null;
  lastGame: string;
  emit(event: string, data: unknown): void;
}

function need<T>(source: (() => Promise<T>) | undefined, what: string): Promise<T> {
  if (!source) return Promise.reject(new Error(`${what} isn't available in this app`));
  return source();
}

function makeMethods(host: BridgeHost): Record<string, Method> {
  return {
    ready: () => null,
    log: (ext, p) => {
      const level = ["log", "info", "warn", "error"].includes(String(p.level)) ? String(p.level) : "log";
      const list = logs.get(ext.id) ?? [];
      list.push({ at: Date.now(), level, text: String(p.text ?? "").slice(0, 500) });
      if (list.length > LOG_LINES) list.splice(0, list.length - LOG_LINES);
      logs.set(ext.id, list);
      return null;
    },
    "app.info": () => {
      const d = extensionData();
      return { app: d.app, version: d.appVersion, locale: host.locale(), theme: host.theme().mode };
    },
    "ext.permissions": (ext) => [...ext.granted],
    "storage.get": (ext, p) => readJson(kvKey(ext.id, "kv"))[String(p.key)] ?? null,
    "storage.keys": (ext) => Object.keys(readJson(kvKey(ext.id, "kv"))),
    "storage.set": (ext, p) => {
      const key = String(p.key ?? "");
      if (!key || key.length > 100) throw new Error("storage keys are 1 to 100 characters");
      const k = kvKey(ext.id, "kv");
      const all = readJson(k);
      all[key] = JSON.parse(JSON.stringify(p.value ?? null));
      const body = JSON.stringify(all);
      if (body.length > STORAGE_CAP) throw new Error("storage is full (512 KB per extension)");
      localStorage.setItem(k, body);
      return null;
    },
    "storage.remove": (ext, p) => {
      const k = kvKey(ext.id, "kv");
      const all = readJson(k);
      delete all[String(p.key)];
      localStorage.setItem(k, JSON.stringify(all));
      return null;
    },
    "settings.get": (ext) => extensionSettings(ext),
    "mods.list": () => need(extensionData().mods, "the mod list"),
    "servers.list": () => need(extensionData().servers, "the server list"),
    "friends.list": () => need(extensionData().friends, "the friends list"),
    "presets.list": () => need(extensionData().presets, "presets"),
    "coach.sessions": () => need(extensionData().coach, "MXB Coach sessions"),
    "studio.open": async (_ext, p) => {
      const screen = String(p.screen ?? "");
      if (!(STUDIO_SCREENS as readonly string[]).includes(screen)) throw new Error("unknown Studio screen");
      const open = extensionData().studio;
      if (!open) throw new Error("Frost's Studio can't be opened from this app");
      await open(screen as StudioScreen);
      return null;
    },
    "game.state": () => need(extensionData().gameState, "game state"),
    "game.subscribe": (ext, _p, f) => {
      if (!ext.granted.includes("game.state")) throw new Error("permission denied: game.state");
      const src = extensionData().gameState;
      if (!src || f.gameTimer !== null) return null;
      const tick = () =>
        void src()
          .then((s) => {
            const key = JSON.stringify(s);
            if (key !== f.lastGame) {
              f.lastGame = key;
              f.emit("game.changed", s);
            }
          })
          .catch(() => {});
      tick();
      f.gameTimer = window.setInterval(tick, 5000);
      return null;
    },
    "ui.toast": (ext, p) => {
      const text = cleanLabel(p.text, 200);
      const kind: ToastKind = p.kind === "success" || p.kind === "error" ? p.kind : "info";
      if (text) host.toast(text, kind, ext);
      return null;
    },
    "ui.openExternal": async (ext, p) => {
      const url = String(p.url ?? "");
      let parsed: URL;
      try {
        parsed = new URL(url);
      } catch {
        throw new Error("not a URL");
      }
      if (parsed.protocol !== "https:" && parsed.protocol !== "http:") throw new Error("only http(s) links open");
      if (parsed.username || parsed.password) throw new Error("links with credentials don't open");
      if (!(await host.confirmOpen(parsed.href, ext))) return false;
      await host.openUrl(parsed.href);
      return true;
    },
    "mcp.result": (ext, p) => {
      const call = toolCalls.get(String(p.callId));
      if (!call || call.ext !== ext.id) return null;
      toolCalls.delete(String(p.callId));
      if (p.error) call.reject(new Error(cleanLabel(p.error, 300)));
      else call.resolve(JSON.parse(JSON.stringify(p.result ?? null)));
      return null;
    },
  };
}

/**
 * Wire one frame to the bridge. Returns a disposer. `main` marks the extension's invisible
 * background page, which is where MCP tool calls go first.
 */
export function attachBridge(
  frame: HTMLIFrameElement,
  extId: string,
  host: BridgeHost,
  opts: { main?: boolean; onBreach?: () => void } = {},
): () => void {
  const methods = makeMethods(host);
  let windowStart = 0;
  let count = 0;

  const emit = (event: string, data: unknown) => {
    // `*` because the frame's origin is opaque and has no name to target. Safe only because
    // the frame cannot navigate away (sandbox + the app's frame-src), and see `onLoad`.
    frame.contentWindow?.postMessage({ mxbext: 1, event, data }, "*");
  };
  const state: FrameState = { gameTimer: null, lastGame: "", emit };
  const handle: Handle = { ext: extId, main: opts.main === true, emit };

  const reply = (id: number, body: { result?: unknown; error?: string }) =>
    frame.contentWindow?.postMessage({ mxbext: 1, id, ...body }, "*");

  const onMessage = (e: MessageEvent) => {
    if (!frame.contentWindow || e.source !== frame.contentWindow) return;
    const m = e.data as { mxbext?: unknown; id?: unknown; method?: unknown; params?: unknown };
    if (!m || m.mxbext !== 1 || typeof m.id !== "number" || typeof m.method !== "string") return;
    const id = m.id;
    const fn = Object.prototype.hasOwnProperty.call(methods, m.method) ? methods[m.method] : undefined;
    if (!fn) return reply(id, { error: `unknown method ${m.method.slice(0, 40)}` });
    const ext = current(extId);
    if (!ext) return reply(id, { error: "this extension is disabled" });
    const perm = METHOD_PERMISSION[m.method];
    if (perm && !ext.granted.includes(perm)) return reply(id, { error: `permission denied: ${perm}` });
    const now = Date.now();
    if (now - windowStart > 1000) {
      windowStart = now;
      count = 0;
    }
    // Console lines don't spend the budget real calls need; the log ring buffer bounds them.
    if (m.method !== "log" && ++count > RATE_PER_SEC) return reply(id, { error: "too many calls" });
    const params = m.params && typeof m.params === "object" ? (m.params as Record<string, unknown>) : {};
    Promise.resolve()
      .then(() => fn(ext, params, state))
      .then(
        (result) => reply(id, { result: result === undefined ? null : result }),
        (err) => reply(id, { error: String(err instanceof Error ? err.message : err).slice(0, 300) }),
      );
  };

  // A srcdoc frame loads once. A second load means it navigated, which the CSP should have
  // stopped; whatever it is now, it is not the extension we vetted, so it gets nothing more.
  let loads = 0;
  const onLoad = () => {
    loads += 1;
    if (loads > 1) {
      dispose();
      frame.removeAttribute("srcdoc");
      frame.src = "about:blank";
      opts.onBreach?.();
      return;
    }
    emit("theme", host.theme());
  };

  window.addEventListener("message", onMessage);
  frame.addEventListener("load", onLoad);
  if (!frames.has(extId)) frames.set(extId, new Set());
  frames.get(extId)!.add(handle);

  let disposed = false;
  function dispose() {
    if (disposed) return;
    disposed = true;
    window.removeEventListener("message", onMessage);
    frame.removeEventListener("load", onLoad);
    if (state.gameTimer !== null) window.clearInterval(state.gameTimer);
    frames.get(extId)?.delete(handle);
  }
  return dispose;
}
