/* global window, fetch */
// Dev only: stands in for Tauri's IPC in a plain browser, forwarding every command to the app's
// windowless bridge (`cargo build --features bridge`, run with CINEMA_BRIDGE_PORT). Inject it
// before the page loads (CDP Page.addScriptToEvaluateOnNewDocument) to drive the real UI
// against the real backend in a headless browser, for screenshots. Never shipped.
(() => {
  const port = window.CINEMA_BRIDGE_PORT || 17470;
  let next = 1;
  const callbacks = new Map();
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    transformCallback(cb, once) {
      const id = next++;
      callbacks.set(id, (v) => {
        if (once) callbacks.delete(id);
        return cb && cb(v);
      });
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    convertFileSrc(p) {
      return p;
    },
    async invoke(cmd, args) {
      // Window chrome and plugins have nothing to do in a page.
      if (cmd.startsWith("plugin:")) return null;
      const r = await fetch(`http://127.0.0.1:${port}/invoke/${cmd}`, {
        method: "POST",
        body: JSON.stringify(args ?? {}),
      });
      const type = r.headers.get("content-type") || "";
      if (!r.ok) throw await r.json().catch(() => r.statusText);
      if (type.includes("octet-stream")) return await r.arrayBuffer();
      return await r.json();
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
})();
