import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * Width to request for a card thumbnail.
 *
 * Cards render around 300 CSS px wide in the four-column grid; 600 covers a 2× display
 * without asking for pixels nobody sees. It matters because neither catalog offers a
 * thumbnail size — mxbikes-shop.com serves 1000–1280px product images, roughly half a
 * megabyte each, so a page of 24 cards was ~12 MB of full-resolution originals.
 */
export const GRID_THUMB_WIDTH = 600;

/** Width for the small strip under the detail gallery. */
export const STRIP_THUMB_WIDTH = 240;

/**
 * Width for images embedded in a description's own markup.
 *
 * The description column is around 600 CSS px, so this covers a 2× display. The store's
 * embedded shots run from 800px to full 1920px screenshots and a single product page can
 * carry seven of them, which is where the ceiling earns its keep.
 */
export const DESCRIPTION_IMAGE_WIDTH = 1200;

/**
 * Route a remote image through the app's on-disk cache.
 *
 * The heavy lifting is in `src-tauri/src/imgcache.rs`; this only builds the URL. Use
 * `convertFileSrc` rather than string interpolation — the custom scheme resolves to
 * `imgcache://localhost/…` on macOS and Linux but `http://imgcache.localhost/…` on Windows,
 * and hand-rolling it is the classic "works on my Mac, blank thumbnails on Windows" bug.
 *
 * Pass `width` for a downscaled copy, or omit it for the original (what the full-size
 * gallery wants). Each width is a separate cache entry.
 *
 * Only the catalogs' and the two stores' URLs are served; anything else is refused by the
 * handler, which surfaces as a normal image error and falls back to the placeholder icon.
 */
export function cachedImage(
  url: string | null | undefined,
  width?: number,
): string | undefined {
  if (!url) return undefined;
  const src = convertFileSrc(url, "imgcache");
  return width ? `${src}?w=${width}` : src;
}

/**
 * Hosts the cache will actually fetch from — the mirror of `ALLOWED_HOSTS` in
 * `src-tauri/src/imgcache.rs`, which is the copy that enforces it.
 *
 * Callers handing over a URL from either catalog don't need this; it's for markup written by
 * mod authors, which can point anywhere. Routing a host the handler refuses would turn an
 * image that loads today into one that can't load at all, so those are left alone.
 */
const CACHEABLE_HOSTS = [
  "mxb-mods.com",
  "gpb-mods.com",
  "mxbikes-shop.com",
  "mxbikes-shop.b-cdn.net",
  "mxb-hub.com",
];

/** Whether [`cachedImage`] would resolve to something the handler is willing to serve. */
export function isCacheableImage(url: string): boolean {
  try {
    const { protocol, hostname } = new URL(url);
    if (protocol !== "https:" && protocol !== "http:") return false;
    const host = hostname.toLowerCase();
    return CACHEABLE_HOSTS.some((h) => host === h || host.endsWith(`.${h}`));
  } catch {
    // A relative or malformed src — not something the cache can be handed.
    return false;
  }
}

/**
 * Whether the `imgcache` scheme has proved unusable this session.
 *
 * A user's report: every picture in the app blank — Browse, Shop and Hub alike — while all
 * the text around them loaded fine. Their log read `thumbnails 0 served (0 MB)` for the whole
 * session, so nothing had reached the handler at all; three different origins failing at once
 * is the scheme, not any one site. Whatever intercepts `http://imgcache.localhost` on that
 * machine, the origin URL still loads in the same webview, so falling back to it is the
 * difference between a grid of pictures and a grid of placeholders.
 *
 * Session-local and one-way. Losing the cache for a session costs re-fetching thumbnails;
 * getting it wrong the other way costs the user every image, which is what they reported.
 */
let directRecoveries = 0;
let schemeBroken = false;

/**
 * How many images must load direct after failing through the cache before the scheme itself
 * is written off. More than one, because a single image can fail through the cache on its own
 * merits — an origin that 404s only for us, a body past the fetch cap, a format the sniffer
 * won't have — and one of those must not cost everyone else their cache.
 */
const RECOVERIES_BEFORE_GIVING_UP = 3;

/** Whether [`cachedImage`] should be skipped in favour of the origin URL. */
export function schemeIsBroken(): boolean {
  return schemeBroken;
}

/**
 * Report that `url` failed through the cache and then loaded directly.
 *
 * Only affects images mounted afterwards; the one that recovered is already showing.
 */
export function noteDirectRecovery(): void {
  directRecoveries += 1;
  if (directRecoveries >= RECOVERIES_BEFORE_GIVING_UP) schemeBroken = true;
}
