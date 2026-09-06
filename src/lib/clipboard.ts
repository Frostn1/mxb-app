/**
 * Copy text to the clipboard, saying whether it worked.
 *
 * The process writes it, not the webview. A share code is copied at the moment its upload
 * finishes — minutes after the click that started it, and often with the window in the
 * background — and both web paths refuse there: the async Clipboard API wants a focused
 * document, `execCommand` wants a live user gesture. Neither survives the wait. The web
 * calls stay behind it for the browser, where there is no process to ask.
 */
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function copyText(text: string): Promise<boolean> {
  try {
    await writeText(text);
    return true;
  } catch {
    // Not in the app, or the plugin is missing: fall back to the webview's own paths.
  }
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(ta);
      return ok;
    } catch {
      return false;
    }
  }
}
