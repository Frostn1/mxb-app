/**
 * Building the document an extension's view runs in.
 *
 * A view is an `<iframe sandbox="allow-scripts" srcdoc=…>`. Each part of that line is a wall:
 *
 *   - **`sandbox` without `allow-same-origin`** gives the frame an opaque origin. It can't
 *     read the app's DOM, storage or cookies, and the app's IPC answers its own origin only.
 *   - **No `allow-top-navigation`, `allow-popups`, `allow-forms`, `allow-modals`**: it can't
 *     move the window, open one, submit anywhere or block the app with `alert()`.
 *   - **A CSP in the document itself**, written here before a byte of the extension's own
 *     HTML: no network unless the manifest lists a host and the person granted `network`,
 *     no frames, no workers, no plugins, no `<base>`.
 *   - **Tauri's own scripts run in the main frame only** (2.x default), so `__TAURI_INTERNALS__`
 *     and the IPC key never exist inside the frame, and a request without them is refused.
 *   - The app's own `frame-src` (in each app's `index.html`) stops the frame navigating
 *     itself somewhere else — the one exit `sandbox` leaves open.
 *
 * The package's files are inlined into `srcdoc` because a srcdoc document has no URL of its
 * own to resolve `script src="app.js"` against, and serving the files over a URL would hand
 * every extension a same-scheme origin to share. One self-contained document per view.
 */

export interface SandboxReader {
  /** A text file from the extension's folder. */
  text(path: string): Promise<string>;
  /** An image from the extension's folder, as a data: URL. */
  asset(path: string): Promise<string>;
}

/** The view's own policy. `network` is the manifest's hosts, and only if `network` is granted. */
export function viewCsp(network: string[]): string {
  const hosts = network.filter((h) => /^https:\/\/[a-z0-9.-]+(?::\d{1,5})?$/.test(h));
  const connect = hosts.length ? hosts.join(" ") : "'none'";
  const img = ["data:", "blob:", ...hosts].join(" ");
  return [
    "default-src 'none'",
    "script-src 'unsafe-inline'",
    "style-src 'unsafe-inline'",
    `img-src ${img}`,
    "font-src data:",
    "media-src data: blob:",
    `connect-src ${connect}`,
    "frame-src 'none'",
    "child-src 'none'",
    "worker-src 'none'",
    "object-src 'none'",
    "manifest-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
  ].join("; ");
}

/** Resolve `href` against the entry file's folder; null for anything that leaves the package. */
export function resolveInPackage(entry: string, href: string): string | null {
  const h = href.trim();
  if (!h || /^[a-z][a-z0-9+.-]*:/i.test(h) || h.startsWith("//") || h.startsWith("/")) return null;
  const clean = h.split(/[?#]/)[0];
  const parts = entry.split("/").slice(0, -1);
  for (const seg of clean.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else parts.push(seg);
  }
  return parts.length ? parts.join("/") : null;
}

/**
 * Assemble the srcdoc for one view: policy first, then the `mxb` client, then the author's
 * document with its local scripts, stylesheets and images inlined. Remote references are
 * dropped, not fetched: the policy would refuse them anyway, and a dropped tag fails visibly.
 */
export async function buildSrcdoc(
  entry: string,
  reader: SandboxReader,
  opts: { network: string[]; themeCss: string; shim: string },
): Promise<string> {
  const html = await reader.text(entry);
  const doc = new DOMParser().parseFromString(html, "text/html");

  // Anything that could change where the document thinks it is, or load another document.
  doc
    .querySelectorAll("meta[http-equiv], base, iframe, frame, frameset, object, embed, portal")
    .forEach((el) => el.remove());

  for (const s of Array.from(doc.querySelectorAll("script[src]"))) {
    const path = resolveInPackage(entry, s.getAttribute("src") ?? "");
    if (!path) {
      s.remove();
      continue;
    }
    const inline = doc.createElement("script");
    const type = s.getAttribute("type");
    if (type) inline.setAttribute("type", type);
    inline.textContent = await reader.text(path);
    s.replaceWith(inline);
  }

  for (const l of Array.from(doc.querySelectorAll("link"))) {
    const rel = (l.getAttribute("rel") ?? "").toLowerCase();
    const path = rel === "stylesheet" ? resolveInPackage(entry, l.getAttribute("href") ?? "") : null;
    if (path) {
      const style = doc.createElement("style");
      style.textContent = await reader.text(path);
      l.replaceWith(style);
    } else {
      // Preload, prefetch, dns-prefetch, icons: every other <link> is a request we didn't ask for.
      l.remove();
    }
  }

  for (const img of Array.from(doc.querySelectorAll("img[src]"))) {
    const path = resolveInPackage(entry, img.getAttribute("src") ?? "");
    if (path) {
      try {
        img.setAttribute("src", await reader.asset(path));
      } catch {
        img.removeAttribute("src");
      }
    }
    img.removeAttribute("srcset");
  }

  const head = doc.head;
  const csp = doc.createElement("meta");
  csp.setAttribute("http-equiv", "Content-Security-Policy");
  csp.setAttribute("content", viewCsp(opts.network));
  const referrer = doc.createElement("meta");
  referrer.setAttribute("name", "referrer");
  referrer.setAttribute("content", "no-referrer");
  const theme = doc.createElement("style");
  theme.setAttribute("id", "mxb-theme");
  theme.textContent = opts.themeCss;
  const shim = doc.createElement("script");
  shim.textContent = opts.shim;
  // Order matters: the policy has to be in force before anything of the author's is parsed.
  head.prepend(csp, referrer, theme, shim);

  return `<!doctype html>\n${doc.documentElement.outerHTML}`;
}

/** The theme tokens a view gets, read from the app's own computed style. */
export const VIEW_TOKENS = [
  "--background",
  "--foreground",
  "--card",
  "--card-foreground",
  "--popover",
  "--popover-foreground",
  "--primary",
  "--primary-foreground",
  "--secondary",
  "--secondary-foreground",
  "--muted",
  "--muted-foreground",
  "--accent",
  "--accent-foreground",
  "--destructive",
  "--success",
  "--warning",
  "--border",
  "--input",
  "--ring",
  "--radius",
];

/** `:root { --x: … }` for a view, from values the app computed. Values are re-checked. */
export function themeCss(vars: Record<string, string>, mode: "light" | "dark"): string {
  const decls = Object.entries(vars)
    .filter(([k, v]) => VIEW_TOKENS.includes(k) && /^[#a-z0-9(),.%\s/+-]{1,80}$/i.test(v) && !/url|expression|image|var\(/i.test(v))
    .map(([k, v]) => `${k}: ${v};`)
    .join(" ");
  const font = vars["font-family"];
  const body = font && /^[a-z0-9\s,"'.()-]{1,240}$/i.test(font) && !/url|expression|image|var\(/i.test(font)
    ? ` body { font-family: ${font}; }`
    : "";
  return `:root { color-scheme: ${mode}; ${decls} }${body}`;
}

/**
 * `window.mxb` inside the frame. Plain ES2019 in a string: it is injected as an inline
 * script, so it can't import anything and must not depend on the app's bundle.
 *
 * It trusts messages from `parent` only, and posts with target `*` because the frame has an
 * opaque origin and cannot name the app's. The host checks `event.source` on its side.
 */
export const SHIM = String.raw`(function () {
  "use strict";
  var parentWin = window.parent;
  var seq = 0;
  var pending = new Map();
  var listeners = new Map();
  var tools = new Map();
  function post(m) { m.mxbext = 1; parentWin.postMessage(m, "*"); }
  function call(method, params) {
    return new Promise(function (resolve, reject) {
      var id = ++seq;
      pending.set(id, { resolve: resolve, reject: reject });
      post({ id: id, method: method, params: params === undefined ? null : params });
    });
  }
  function on(event, fn) {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(fn);
    return function () { listeners.get(event).delete(fn); };
  }
  function emit(event, data) {
    var set = listeners.get(event);
    if (!set) return;
    set.forEach(function (fn) { try { fn(data); } catch (e) { console.error(e); } });
  }
  function applyTheme(t) {
    var el = document.getElementById("mxb-theme");
    if (el && typeof t.css === "string") el.textContent = t.css;
    document.documentElement.classList.remove("light", "dark");
    document.documentElement.classList.add(t.mode === "light" ? "light" : "dark");
  }
  window.addEventListener("message", function (e) {
    if (e.source !== parentWin) return;
    var m = e.data;
    if (!m || m.mxbext !== 1) return;
    if (typeof m.id === "number" && pending.has(m.id)) {
      var p = pending.get(m.id);
      pending.delete(m.id);
      if (m.error) p.reject(new Error(m.error)); else p.resolve(m.result);
      return;
    }
    if (typeof m.event !== "string") return;
    if (m.event === "theme") applyTheme(m.data || {});
    if (m.event === "command" && m.data) emit("command:" + m.data.id, m.data);
    if (m.event === "mcp.call" && m.data) {
      var d = m.data;
      var fn = tools.get(d.name);
      Promise.resolve()
        .then(function () { if (!fn) throw new Error("no handler for " + d.name); return fn(d.args || {}); })
        .then(
          function (r) { call("mcp.result", { callId: d.callId, result: r === undefined ? null : r }); },
          function (err) { call("mcp.result", { callId: d.callId, error: String((err && err.message) || err) }); }
        );
    }
    emit(m.event, m.data);
  });
  var api = {
    version: 1,
    call: call,
    on: on,
    app: { info: function () { return call("app.info"); } },
    permissions: function () { return call("ext.permissions"); },
    storage: {
      get: function (k) { return call("storage.get", { key: k }); },
      set: function (k, v) { return call("storage.set", { key: k, value: v }); },
      remove: function (k) { return call("storage.remove", { key: k }); },
      keys: function () { return call("storage.keys"); }
    },
    settings: {
      get: function () { return call("settings.get"); },
      onChange: function (fn) { return on("settings.changed", fn); }
    },
    mods: { list: function () { return call("mods.list"); } },
    servers: { list: function () { return call("servers.list"); } },
    friends: { list: function () { return call("friends.list"); } },
    game: {
      state: function () { return call("game.state"); },
      onChange: function (fn) { call("game.subscribe").catch(function () {}); return on("game.changed", fn); }
    },
    presets: { list: function () { return call("presets.list"); } },
    coach: { sessions: function () { return call("coach.sessions"); } },
    studio: { open: function (screen) { return call("studio.open", { screen: screen }); } },
    ui: {
      toast: function (text, kind) { return call("ui.toast", { text: text, kind: kind || "info" }); },
      openExternal: function (url) { return call("ui.openExternal", { url: url }); }
    },
    commands: { on: function (id, fn) { return on("command:" + id, fn); } },
    mcp: { handle: function (name, fn) { tools.set(name, fn); } }
  };
  Object.freeze(api.app); Object.freeze(api.storage); Object.freeze(api.settings);
  Object.freeze(api.mods); Object.freeze(api.servers); Object.freeze(api.friends);
  Object.freeze(api.game); Object.freeze(api.presets); Object.freeze(api.coach); Object.freeze(api.studio); Object.freeze(api.ui);
  Object.freeze(api.commands); Object.freeze(api.mcp);
  window.mxb = Object.freeze(api);
  // The frame's console and uncaught errors go to the app, for extension_logs. Fire and
  // forget, through the original console so a failing log can't loop.
  var original = { log: console.log, info: console.info, warn: console.warn, error: console.error };
  function send(level, args) {
    try {
      var text = Array.prototype.map.call(args, function (a) {
        if (a instanceof Error) return a.stack || a.message;
        if (typeof a === "string") return a;
        try { return JSON.stringify(a); } catch (e) { return String(a); }
      }).join(" ");
      post({ id: ++seq, method: "log", params: { level: level, text: text.slice(0, 500) } });
    } catch (e) {}
  }
  ["log", "info", "warn", "error"].forEach(function (level) {
    console[level] = function () { send(level, arguments); return original[level].apply(console, arguments); };
  });
  window.addEventListener("error", function (e) { send("error", [e.message + " (" + (e.filename || "") + ":" + e.lineno + ")"]); });
  window.addEventListener("unhandledrejection", function (e) { send("error", ["unhandled: " + (e.reason && e.reason.message || e.reason)]); });
  call("ready").catch(function () {});
})();`;
