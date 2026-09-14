/**
 * Where a person lands when Steam sends them back: mxbsecure.com's /steam page, with the
 * site's own nav and footer. Only a code goes in the URL; the site holds the words, so a
 * link can't put text of its own on our page.
 */

export type SteamResult = "linked" | "already-linked" | "expired" | "other-browser" | "unconfirmed" | "busy" | "unavailable";

export function steamResult(site: string, result: SteamResult): Response {
  return new Response(null, {
    status: 303,
    headers: { Location: `${site}/steam?r=${result}`, "Cache-Control": "no-store" },
  });
}

/** mxbsecure.com's favicon: the mono "m" on black. */
const FAVICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#0b0b0c"/>' +
  '<path fill="#fff" transform="translate(12.43 49.93) scale(0.06522 -0.06522)" d="M24 0V538H146L149 456Q162 495 181 518Q209 550 251 550Q301 550 323 517Q338 494 347 454Q360 493 383 516Q416 550 464 550Q521 550 548.5 508.0Q576 466 576 379V0H434V346Q434 393 427.0 407.5Q420 422 404 422Q392 422 383.5 414.5Q375 407 369.5 390.0Q364 373 364 346V0H236V346Q236 390 228.5 406.0Q221 422 204 422Q192 422 183.0 414.5Q174 407 170.0 390.0Q166 373 166 346V0Z"/></svg>';

const MONO = `"Geist Mono",ui-monospace,SFMono-Regular,Menlo,monospace`;

/** mxbsecure.com's look: wordmark, bold mono headline, one line on a card, light and dark. */
const STYLE =
  `:root{color-scheme:light dark;--bg:#f5f5f7;--card:#fff;--fg:#1d1d1f;--muted:#6e6e73;--line:rgba(0,0,0,.08);--blue:#0071e3}` +
  `@media (prefers-color-scheme:dark){:root{--bg:#000;--card:#1c1c1e;--fg:#f5f5f7;--muted:#a1a1a6;--line:rgba(255,255,255,.1);--blue:#2997ff}}` +
  `*{box-sizing:border-box}` +
  `body{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;background:var(--bg);color:var(--fg);` +
  `font:17px/1.5 -apple-system,BlinkMacSystemFont,"SF Pro Text","Helvetica Neue",Arial,sans-serif}` +
  `main{width:100%;max-width:440px;padding:40px 36px;text-align:center;background:var(--card);border:1px solid var(--line);border-radius:28px}` +
  `.mark{margin:0 0 32px;font:800 20px ${MONO};letter-spacing:-.02em}` +
  `h1{margin:0 0 12px;font:800 32px/1.15 ${MONO};letter-spacing:-.03em}` +
  `.msg{margin:0;color:var(--muted)}`;

/** Text or an attribute value, safe inside HTML. */
const escapeHtml = (s: string) =>
  s.replace(/[<>&"']/g, (c) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;", '"': "&quot;", "'": "&#39;" })[c]!);

/**
 * The same branded card, shown for a beat before bouncing onward to `to` — the mxbsecure moment
 * on the way *into* Steam, so the sign-in reads mxbsecure → Steam → mxbsecure rather than jumping
 * a player straight to a Steam URL. A `<meta refresh>` does the redirect with no script (so it
 * works under any CSP), and a plain link is there if it doesn't fire.
 */
export function redirectPage(to: string, title: string, message: string): Response {
  const href = escapeHtml(to);
  const body =
    `<!doctype html><html lang="en"><meta charset="utf-8">` +
    `<meta name="viewport" content="width=device-width,initial-scale=1">` +
    `<meta http-equiv="refresh" content="1;url=${href}">` +
    `<title>mxbsecure</title>` +
    `<link rel="icon" href="data:image/svg+xml,${encodeURIComponent(FAVICON)}">` +
    `<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist+Mono:wght@800&display=swap">` +
    `<style>${STYLE} .go{display:inline-block;margin-top:20px;color:var(--blue);text-decoration:none}</style>` +
    `<main><p class="mark">mxbsecure</p><h1>${escapeHtml(title)}</h1><p class="msg">${escapeHtml(message)}</p>` +
    `<a class="go" href="${href}">Continue to Steam →</a></main>`;
  return new Response(body, { status: 200, headers: { "content-type": "text/html; charset=utf-8" } });
}
