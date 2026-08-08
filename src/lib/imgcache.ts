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
