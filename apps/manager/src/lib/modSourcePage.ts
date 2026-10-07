/**
 * The mod's own page on the catalog site, which is what "View on site" opens and what
 * "Copy link" copies.
 *
 * The page's own permalink is preferred; the slug on the game's catalog domain is the
 * fallback for a detail that carries none. Anything that is not an http(s) URL is ignored, so
 * a malformed value from the site can never reach the shell opener.
 */
export function modSourcePageUrl(
  link: string | null | undefined,
  catalogDomain: string,
  slug: string,
): string {
  const l = (link ?? "").trim();
  if (/^https?:\/\//i.test(l)) return l;
  return `https://${catalogDomain}/${slug}/`;
}
