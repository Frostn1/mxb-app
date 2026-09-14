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

/**
 * The same branded card, shown for a beat before bouncing onward to `to` — the mxbsecure moment
 * on the way *into* Steam, so the sign-in reads mxbsecure → Steam → mxbsecure rather than jumping
 * a player straight to a Steam URL. A `<meta refresh>` does the redirect with no script (so it
 * works under any CSP), and a plain link is there if it doesn't fire.
 */
export function redirectPage(to: string, title: string, message: string): Response {
  const href = escape(to);
  const body =
    `<!doctype html><html lang="en"><meta charset="utf-8">` +
    `<meta name="viewport" content="width=device-width,initial-scale=1">` +
    `<meta http-equiv="refresh" content="1;url=${href}">` +
    `<title>mxbsecure</title>` +
    `<link rel="icon" href="data:image/svg+xml,${encodeURIComponent(FAVICON)}">` +
    `<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist+Mono:wght@800&display=swap">` +
    `<style>${STYLE} .go{display:inline-block;margin-top:20px;color:var(--blue);text-decoration:none}</style>` +
    `<main><p class="mark">mxbsecure</p><h1>${escape(title)}</h1><p class="msg">${escape(message)}</p>` +
    `<a class="go" href="${href}">Continue to Steam →</a></main>`;
  return new Response(body, { status: 200, headers: { "content-type": "text/html; charset=utf-8" } });
}
