import { useMemo } from "react";
import { open } from "@tauri-apps/plugin-shell";
import {
  cachedImage,
  DESCRIPTION_IMAGE_WIDTH,
  isCacheableImage,
} from "../../lib/imgcache";

interface RichDescriptionProps {
  /** Sanitised in Rust before it reaches here. */
  html: string;
}

/**
 * Site-authored HTML — an mxb-mods post body or an mxbikes-shop product description —
 * rendered as HTML.
 *
 * Two things happen to the markup on the way in, and both exist because this is a webview
 * pretending to be an app rather than a browser tab:
 *
 * **Images are re-pointed at the cache.** `cdn.mxbikes-shop.com` refuses any request whose
 * `Referer` isn't the store itself, and the webview sends its own origin — `tauri://localhost`
 * in a release build, `http://localhost:1420` in dev — so an `<img>` written by the store
 * 403s in place and the description renders as a column of broken frames. Routing them
 * through `imgcache://` fetches them from Rust, which sends no referer at all and is the same
 * path the grid and gallery already use, so they also land in the on-disk cache instead of
 * being re-downloaded on every visit.
 *
 * **Links open in the user's browser.** These descriptions carry real links (the shop's alone
 * hold ~1250, pointing at Discord, YouTube and the store), and this webview has no chrome: an
 * in-page navigation would replace the entire app with a website and leave no way back,
 * because there is no back button to come back with.
 *
 * The click is delegated from the container rather than bound per link: the markup is
 * injected, so there are no React elements to attach handlers to, and one listener covers
 * whatever the store publishes next.
 *
 * The HTML itself is trusted only as far as `sanitize_html` in `mods/shop_catalog.rs` (and
 * the equivalent for mxb-mods) makes it trustworthy — this component adds no sanitising of
 * its own, and must not be handed markup that skipped it.
 */
export default function RichDescription({ html }: RichDescriptionProps) {
  const withCachedImages = useMemo(() => proxyImages(html), [html]);

  return (
    <div
      className="mod-description"
      onClick={(e) => {
        const anchor = (e.target as HTMLElement).closest?.("a");
        const href = anchor?.getAttribute("href")?.trim();
        if (!anchor) return;
        // Swallow the click whatever the href turns out to be: an anchor we can't hand to
        // the browser is still an anchor that must not navigate the app.
        e.preventDefault();
        if (href && /^https?:\/\//i.test(href)) void open(href);
      }}
      dangerouslySetInnerHTML={{ __html: withCachedImages }}
    />
  );
}

/**
 * Point every embedded image at `imgcache://`.
 *
 * Parsed rather than regex-replaced: this markup is written by hand by dozens of different
 * mod authors, and a pattern that works on `<img src="…">` meets `<img\n  data-src='…'  >`
 * on the next product. `DOMParser` runs no scripts and fetches nothing — it only builds a
 * tree — so this is a rewrite, not a render.
 *
 * A host the cache won't serve is left exactly as it was — see [`isCacheableImage`]. Those
 * images aren't hotlink-protected as far as we know, so a direct load is their best chance;
 * handing them to a handler that refuses them would break the ones that work today.
 */
function proxyImages(html: string): string {
  if (typeof DOMParser === "undefined") return html;

  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const img of doc.querySelectorAll("img[src]")) {
    const src = img.getAttribute("src");
    if (!src || !isCacheableImage(src)) continue;
    const cached = cachedImage(src, DESCRIPTION_IMAGE_WIDTH);
    if (!cached) continue;
    img.setAttribute("src", cached);
    // `srcset` outranks `src`, so leaving one behind would quietly undo the rewrite.
    img.removeAttribute("srcset");
    img.setAttribute("loading", "lazy");
  }
  return doc.body.innerHTML;
}
