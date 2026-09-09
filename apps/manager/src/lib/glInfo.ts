import type * as THREE from "three";
import { logClient } from "../api/mods";

/**
 * Renderer names that mean no GPU is involved — the context is a CPU rasteriser standing in
 * for one, at a fraction of the speed.
 */
const SOFTWARE = [
  // Chromium's own, which WebView2 falls back to when the driver is missing or blocklisted.
  "swiftshader",
  // Mesa's, the same fallback on Linux.
  "llvmpipe",
  "softpipe",
  // Microsoft's, on a machine with no usable display driver.
  "basic render driver",
  "warp",
  "software adapter",
];

/** The adapter is a property of the machine, not of the canvas — one report says it. */
let reported = false;

/**
 * Put the GPU this WebGL context actually landed on into the app log.
 *
 * Both viewers draw through WebGL, which is only fast if the webview got a hardware
 * context. A blocklisted driver drops it to a software rasteriser silently — same picture,
 * a fraction of the frame rate — and nothing in a bug report would say so.
 */
export function reportRenderer(gl: THREE.WebGLRenderer, label: string): void {
  if (reported) return;
  reported = true;
  try {
    const ctx = gl.getContext();
    // WebKit removed this extension, so on macOS/Linux the names come back masked
    // ("WebKit WebGL"). Chromium — every Windows install — still answers it.
    const dbg = ctx.getExtension("WEBGL_debug_renderer_info");
    const vendor = String(
      ctx.getParameter(dbg ? dbg.UNMASKED_VENDOR_WEBGL : ctx.VENDOR) ?? "?",
    );
    const renderer = String(
      ctx.getParameter(dbg ? dbg.UNMASKED_RENDERER_WEBGL : ctx.RENDERER) ?? "?",
    );
    const software = SOFTWARE.some((s) => renderer.toLowerCase().includes(s));
    const line = [
      `${label}: WebGL${gl.capabilities.isWebGL2 ? "2" : "1"}`,
      `${vendor} / ${renderer}${dbg ? "" : " (masked)"}`,
      `max texture ${ctx.getParameter(ctx.MAX_TEXTURE_SIZE)}px`,
      `anisotropy ${gl.capabilities.getMaxAnisotropy()}`,
      software ? "SOFTWARE RENDERING — no GPU" : "hardware",
    ].join(", ");
    if (software) console.warn(`[gl] ${line}`);
    else console.info(`[gl] ${line}`);
    void logClient(software ? "warn" : "info", `gl — ${line}`).catch(() => {});
  } catch (e) {
    console.warn("[gl] could not read the renderer:", e);
  }
}
