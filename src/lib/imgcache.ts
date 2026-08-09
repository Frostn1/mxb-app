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
 * Only mxb-mods.com and mxbikes-shop.com URLs are served; anything else is refused by the
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
