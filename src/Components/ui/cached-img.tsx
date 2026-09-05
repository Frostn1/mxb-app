import { useState, type ImgHTMLAttributes } from "react";
import {
  cachedImage,
  noteDirectRecovery,
  schemeIsBroken,
} from "../../lib/imgcache";

interface CachedImgProps extends Omit<ImgHTMLAttributes<HTMLImageElement>, "src" | "width"> {
  /** The origin URL. Routed through the cache; see `cachedImage`. */
  src: string;
  /** Width to ask the cache for, or omit for the original. */
  width?: number;
  /** Called when the origin URL fails too — there is no picture to show. */
  onUnavailable?: () => void;
}

/**
 * A thumbnail, through the on-disk cache, falling back to the origin URL.
 *
 * The cache is worth having — a grid scrolled twice shouldn't refetch every image across the
 * internet — but it sits between the picture and the screen, and a user turned up for whom
 * that middle step never worked at all: every image in the app blank, `thumbnails 0 served`,
 * every catalog's text loading normally around them. Nothing in the app could recover from
 * that, because a cache miss went straight to the placeholder icon.
 *
 * So a miss now tries the origin URL before giving up. That request is the webview's own —
 * real browser, real fingerprint, the site's cookies — which is also the one thing that gets
 * a Cloudflare-challenged user their mxb-mods.com pictures, since a `cf_clearance` cannot be
 * replayed into our HTTP client (see `mxb_session.rs`). One fallback, two failures answered.
 */
export default function CachedImg({
  src,
  width,
  onUnavailable,
  ...rest
}: CachedImgProps) {
  // `direct` from the start once the scheme has been written off, so the rest of a session
  // doesn't pay a doomed request per image.
  const [direct, setDirect] = useState(() => schemeIsBroken());
  // Set when this image is showing *because* of the fallback, so its load can be reported.
  const [recovered, setRecovered] = useState(false);

  return (
    <img
      {...rest}
      // Keyed so React swaps the element rather than reusing one the browser has already
      // marked as errored, which can leave the new `src` unrequested.
      key={direct ? "direct" : "cached"}
      src={direct ? src : cachedImage(src, width)}
      onError={() => {
        if (direct) {
          onUnavailable?.();
          return;
        }
        setDirect(true);
        setRecovered(true);
      }}
      onLoad={() => {
        if (recovered) {
          setRecovered(false);
          noteDirectRecovery();
        }
      }}
    />
  );
}
