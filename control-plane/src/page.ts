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
