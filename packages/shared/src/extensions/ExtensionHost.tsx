import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { ask } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { useI18n } from "../i18n/context";
import { listen } from "@tauri-apps/api/event";
import { extCheckUpdates, extInspectFile, extReadAsset, extReadText, mcpReply } from "./api";
import { InstallDialog } from "./ExtensionsPage";
import type { InspectedPackage } from "./manifest";
import { attachBridge, callExtensionTool, emitAll, extensionLogs, extensionTools, type BridgeHost } from "./bridge";
import { APPEARANCE_ATTRS, compileAppearance, fontFamilyName, type AppearanceInput } from "./appearance";
import { cleanLabel, list, PLAY_ICONS, type ExtView, type InstalledExtension, type PlayIcon } from "./manifest";
import { buildSrcdoc, SHIM, themeCss, VIEW_TOKENS } from "./sandbox";
import { enabledExtensions, parsePick, refreshExtensions, useExtensions } from "./store";

/** The app's theme as a view gets it: the computed tokens, so colorways and picks carry over. */
function currentTheme(): { css: string; mode: "light" | "dark" } {
  const root = document.documentElement;
  const cs = getComputedStyle(root);
  const vars: Record<string, string> = {};
  for (const k of VIEW_TOKENS) {
    const v = cs.getPropertyValue(k).trim();
    if (v) vars[k] = v;
  }
  vars["font-family"] = getComputedStyle(document.body).fontFamily.trim();
  const mode = root.classList.contains("light") ? "light" : "dark";
  return { css: themeCss(vars, mode), mode };
}

/**
 * The app's light/dark, live. A frame whose `color-scheme` differs from its element's is
 * painted on an opaque canvas, which would hide the app's background behind every tab.
 */
function useColorMode(): "light" | "dark" {
  const read = () => (document.documentElement.classList.contains("light") ? "light" : "dark");
  const [mode, setMode] = useState<"light" | "dark">(read);
  useEffect(() => {
    const obs = new MutationObserver(() => setMode(read()));
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => obs.disconnect();
  }, []);
  return mode;
}

/** One host per app: toasts, link confirmation and the theme, for every frame. */
function useBridgeHost(): BridgeHost {
  const { resolved, t } = useI18n();
  const locale = useRef(resolved);
  locale.current = resolved;
  const tr = useRef(t);
  tr.current = t;
  return useMemo<BridgeHost>(
    () => ({
      toast: (text, kind, ext) => {
        const opts = { description: ext.manifest.name };
        if (kind === "success") toast.success(text, opts);
        else if (kind === "error") toast.error(text, opts);
        else toast(text, opts);
      },
      confirmOpen: (url, ext) =>
        ask(tr.current("ext.openLinkBody", { name: ext.manifest.name, url }), {
          title: tr.current("ext.openLinkTitle"),
          kind: "warning",
        }),
      openUrl: (url) => openUrl(url),
      theme: currentTheme,
      locale: () => locale.current,
    }),
    [],
  );
}

/**
 * One sandboxed page of an extension. See `sandbox.ts` for what each attribute is for —
 * none of them are decoration, and `allow-same-origin` must never be added.
 */
export function ExtensionFrame({
  ext,
  entry,
  title,
  main = false,
  className,
}: {
  ext: InstalledExtension;
  entry: string;
  title: string;
  main?: boolean;
  className?: string;
}) {
  const host = useBridgeHost();
  const { t } = useI18n();
  const mode = useColorMode();
  const ref = useRef<HTMLIFrameElement>(null);
  const [doc, setDoc] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const network = ext.granted.includes("network") ? (ext.manifest.network ?? []) : [];
  const netKey = network.join(" ");

  useEffect(() => {
    let live = true;
    setDoc(null);
    setError(null);
    buildSrcdoc(
      entry,
      { text: (p) => extReadText(ext.id, p), asset: (p) => extReadAsset(ext.id, p) },
      { network: netKey ? netKey.split(" ") : [], themeCss: host.theme().css, shim: SHIM },
    )
      .then((d) => live && setDoc(d))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [ext.id, ext.version, entry, netKey, host]);

  useLayoutEffect(() => {
    if (!doc || !ref.current) return;
    return attachBridge(ref.current, ext.id, host, {
      main,
      onBreach: () => setError(t("ext.breach", { name: ext.manifest.name })),
    });
  }, [doc, ext.id, ext.manifest.name, host, main, t]);

  if (error) {
    return <div className="p-6 text-[12.5px] text-destructive">{error}</div>;
  }
  if (!doc) return null;
  return (
    <iframe
      ref={ref}
      title={title}
      // The whole security model of a view is this attribute. No allow-same-origin, ever.
      sandbox="allow-scripts"
      srcDoc={doc}
      referrerPolicy="no-referrer"
      allow="camera 'none'; microphone 'none'; geolocation 'none'; display-capture 'none'; clipboard-read 'none'; clipboard-write 'none'; usb 'none'; serial 'none'; hid 'none'; bluetooth 'none'; payment 'none'"
      className={className ?? "h-full w-full border-0 bg-transparent"}
      style={{ colorScheme: mode }}
    />
  );
}

/** Every view of every enabled extension, for the shell's rail. */
export function useExtensionViews(): { ext: InstalledExtension; view: ExtView; route: string }[] {
  const s = useExtensions();
  return useMemo(
    () =>
      enabledExtensions(s).flatMap((ext) =>
        list(ext.manifest, "views")
          .filter((v) => v && typeof v.id === "string" && typeof v.entry === "string")
          .map((view) => ({ ext, view, route: `ext:${ext.id}/${view.id}` })),
      ),
    [s],
  );
}

const fontCache = new Map<string, string>();

async function registerFont(ext: InstalledExtension, fontId: string): Promise<string | undefined> {
  const def = list(ext.manifest, "fonts").find((f) => f?.id === fontId);
  if (!def || typeof def.src !== "string") return undefined;
  const family = fontFamilyName(ext.id, fontId);
  const cacheKey = `${ext.id}@${ext.version}/${fontId}`;
  if (fontCache.get(family) === cacheKey) return family;
  const url = await extReadAsset(ext.id, def.src);
  const bytes = Uint8Array.from(atob(url.slice(url.indexOf(",") + 1)), (c) => c.charCodeAt(0));
  const weight = typeof def.weight === "string" && /^\d{3}( \d{3})?$/.test(def.weight) ? def.weight : "400";
  const face = new FontFace(family, bytes.buffer, { weight, style: def.style === "italic" ? "italic" : "normal" });
  await face.load();
  document.fonts.add(face);
  fontCache.set(family, cacheKey);
  return family;
}

/**
 * Applies the picked extension looks to this window: one generated `<style>` and a few
 * `data-ext-*` attributes on `<html>`. Mount once per app, inside the theme provider.
 */
export function ExtensionAppearance() {
  const s = useExtensions();

  useEffect(() => {
    brandAssetReader = extReadAsset;
    void refreshExtensions();
  }, []);

  useEffect(() => {
    let live = true;
    const enabled = enabledExtensions(s);
    const find = (pick: string | null) => {
      const p = parsePick(pick);
      const ext = p ? enabled.find((e) => e.id === p.ext) : undefined;
      return p && ext ? { ext, item: p.item } : null;
    };

    void (async () => {
      const input: AppearanceInput = {};
      const theme = find(s.picks.theme);
      const themeDef = theme ? list(theme.ext.manifest, "themes").find((t) => t?.id === theme.item) : undefined;
      if (theme && themeDef) input.theme = { ext: theme.ext.id, theme: themeDef };

      // A pick of its own wins over what the theme brings along.
      const btn = find(s.picks.buttons) ?? (theme && themeDef?.buttonStyle ? { ext: theme.ext, item: themeDef.buttonStyle } : null);
      if (btn) input.buttons = list(btn.ext.manifest, "buttonStyles").find((b) => b?.id === btn.item) ?? null;

      const bg = find(s.picks.background) ?? (theme && themeDef?.background ? { ext: theme.ext, item: themeDef.background } : null);
      if (bg) {
        const def = list(bg.ext.manifest, "backgrounds").find((b) => b?.id === bg.item);
        if (def) {
          const image = typeof def.image === "string" ? await extReadAsset(bg.ext.id, def.image).catch(() => null) : null;
          input.background = { def, image };
        }
      }

      if (theme && themeDef?.fonts) {
        const fonts: AppearanceInput["fonts"] = {};
        if (typeof themeDef.fonts.body === "string") fonts.body = await registerFont(theme.ext, themeDef.fonts.body).catch(() => undefined);
        if (typeof themeDef.fonts.display === "string") fonts.display = await registerFont(theme.ext, themeDef.fonts.display).catch(() => undefined);
        input.fonts = fonts;
      }
      if (themeDef?.play && typeof themeDef.play === "object") input.play = themeDef.play;
      if (themeDef?.components && typeof themeDef.components === "object") input.components = themeDef.components;
      if (!live) return;

      const { css, attrs } = compileAppearance(input);
      let el = document.getElementById("mxb-ext-appearance") as HTMLStyleElement | null;
      if (!el) {
        el = document.createElement("style");
        el.id = "mxb-ext-appearance";
        document.head.appendChild(el);
      }
      el.textContent = css;
      const root = document.documentElement;
      for (const a of APPEARANCE_ATTRS) {
        if (a in attrs) root.setAttribute(a, attrs[a]);
        else root.removeAttribute(a);
      }
    })();
    return () => {
      live = false;
    };
  }, [s]);

  return null;
}

export interface ExtensionBrand {
  /** A data: URL from the theme's own package, or null. */
  logo: string | null;
  wordmark: string | null;
  playLabel: string | null;
  playIcon: PlayIcon | null;
}

const NO_BRAND: ExtensionBrand = { logo: null, wordmark: null, playLabel: null, playIcon: null };
const MAX_LOGO_BYTES = 512 * 1024;

/**
 * What the picked extension theme says about the title bar and the Play button: the parts
 * that are text and pictures rather than CSS. Read by `Brand` and the Play button; empty in
 * any app or state without an extension theme, so they render exactly as before.
 */
// How the brand reads an extension's logo. It is handed in by `ExtensionAppearance`, which only
// the app that hosts extensions mounts: `Brand` sits in every app's title bar, and a direct call
// to `extReadAsset` here would put `ext_read_asset` into the bundle of apps (Coach, Studio) that
// register no extension commands at all.
let brandAssetReader: ((id: string, path: string) => Promise<string>) | null = null;

export function useExtensionBrand(): ExtensionBrand {
  const s = useExtensions();
  const [brand, setBrand] = useState<ExtensionBrand>(NO_BRAND);
  useEffect(() => {
    let live = true;
    const p = parsePick(s.picks.theme);
    const ext = p ? enabledExtensions(s).find((e) => e.id === p.ext) : undefined;
    const theme = ext ? list(ext.manifest, "themes").find((t) => t?.id === p!.item) : undefined;
    if (!ext || !theme) {
      setBrand(NO_BRAND);
      return;
    }
    const wordmark = cleanLabel(theme.brand?.wordmark, 24) || null;
    const playLabel = cleanLabel(theme.play?.label, 12) || null;
    const playIcon = (PLAY_ICONS as readonly string[]).includes(theme.play?.icon ?? "") ? (theme.play!.icon as PlayIcon) : null;
    setBrand({ logo: null, wordmark, playLabel, playIcon });
    const logo = theme.brand?.logo;
    if (typeof logo === "string" && /\.(png|webp|jpe?g|gif)$/i.test(logo)) {
      void (brandAssetReader ? brandAssetReader(ext.id, logo) : Promise.reject(new Error("no extension host")))
        .then((url) => {
          // base64 is 4/3 of the bytes; a logo is a mark, not a banner.
          if (live && url.length * 0.75 <= MAX_LOGO_BYTES) setBrand((b) => ({ ...b, logo: url }));
        })
        .catch(() => {});
    }
    return () => {
      live = false;
    };
  }, [s]);
  return brand;
}

const UPDATE_EVERY_MS = 6 * 60 * 60 * 1000;

/**
 * What runs for extensions while the app is open: each enabled extension's invisible `main`
 * page, theme changes forwarded to every frame, and the auto-update check.
 */
export function ExtensionRuntime({ onOpen }: { onOpen?: (route: string) => void } = {}) {
  const s = useExtensions();
  const snap = useRef(s);
  snap.current = s;
  const openRef = useRef(onOpen);
  openRef.current = onOpen;
  // An agent's install, waiting on the person: the package and how to answer the agent.
  const [agentInstall, setAgentInstall] = useState<{ pkg: InspectedPackage; done: (r: unknown) => void } | null>(null);

  // Requests from the MCP bridge (extmcp.rs): only the webview can show a dialog, open a tab,
  // reach an extension's frames or read their consoles.
  useEffect(() => {
    const off = listen<{ id: number; op: string; params: Record<string, unknown> }>("ext-mcp", (e) => {
      const { id, op, params } = e.payload;
      const ok = (r: unknown) => void mcpReply(id, true, r ?? null).catch(() => {});
      const fail = (err: unknown) => void mcpReply(id, false, String(err instanceof Error ? err.message : err)).catch(() => {});
      try {
        if (op === "tools") return ok(extensionTools());
        if (op === "logs") return ok(extensionLogs(String(params.id)));
        if (op === "refresh") return void refreshExtensions().then(() => ok(true), fail);
        if (op === "call") {
          return void callExtensionTool(String(params.ext), String(params.tool), params.args ?? {}).then(ok, fail);
        }
        if (op === "open") {
          const ext = snap.current.installed.find((x) => x.id === params.id && x.enabled && !x.suspended);
          if (!ext) return fail(`${String(params.id)} isn't installed and enabled`);
          const views = list(ext.manifest, "views");
          const view = views.find((v) => v.id === params.view) ?? views[0];
          if (!view) return fail(`${ext.id} has no tabs`);
          if (!openRef.current) return fail("this app can't open extension tabs");
          openRef.current(`ext:${ext.id}/${view.id}`);
          return ok({ opened: `${ext.id}/${view.id}` });
        }
        if (op === "install") {
          return void extInspectFile(String(params.path)).then(
            (pkg) => setAgentInstall({ pkg, done: ok }),
            fail,
          );
        }
        fail(`unknown request ${op}`);
      } catch (err) {
        fail(err);
      }
    });
    return () => void off.then((f) => f());
  }, []);
  const { t } = useI18n();
  const tr = useRef(t);
  tr.current = t;

  // Theme changes reach views as they happen, the same way the overlay follows Settings.
  useEffect(() => {
    let raf = 0;
    const obs = new MutationObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => emitAll("theme", currentTheme()));
    });
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style", "data-colorway", ...APPEARANCE_ATTRS],
    });
    return () => {
      cancelAnimationFrame(raf);
      obs.disconnect();
    };
  }, []);

  useEffect(() => {
    const check = () =>
      void extCheckUpdates()
        .then(async (results) => {
          for (const r of results) {
            if (r.status === "updated") toast.success(tr.current("ext.updated", { id: r.id, version: r.version ?? "" }));
            if (r.status === "needs-approval") toast(tr.current("ext.updateNeedsApproval", { id: r.id }));
          }
          if (results.some((r) => r.status === "updated" || r.status === "needs-approval")) await refreshExtensions();
        })
        .catch(() => {});
    const first = window.setTimeout(check, 30_000);
    const every = window.setInterval(check, UPDATE_EVERY_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
    };
  }, []);

  const mains = enabledExtensions(s).filter((e) => typeof e.manifest.contributes?.main === "string");
  return (
    <>
    {agentInstall && (
      <InstallDialog
        pkg={agentInstall.pkg}
        onInstalled={(id) => agentInstall.done({ installed: id, granted: snap.current.installed.find((x) => x.id === id)?.granted ?? [] })}
        onClose={() => {
          // Closing after a successful install has already answered; this only reports a cancel.
          agentInstall.done({ installed: null, reason: "the person closed the install dialog" });
          setAgentInstall(null);
        }}
      />
    )}
    <div aria-hidden style={{ display: "none" }}>
      {mains.map((e) => (
        <ExtensionFrame
          key={`${e.id}@${e.version}`}
          ext={e}
          entry={e.manifest.contributes!.main!}
          title={e.manifest.name}
          main
        />
      ))}
    </div>
    </>
  );
}
