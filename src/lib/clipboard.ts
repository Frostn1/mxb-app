/**
 * Copy text to the clipboard, saying whether it worked.
 *
 * The async Clipboard API is the right call and fails anyway in a webview that hasn't
 * focused the document — which is exactly the moment a player clicks "Copy code". The
 * `execCommand` fallback is deprecated everywhere and still the one that lands there, so
 * both paths stay until the webviews agree.
 */
export async function copyText(text: string): Promise<boolean> {
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
