import { useEffect, useRef, useState, type ImgHTMLAttributes } from "react";
import {
  CACHE_PATIENCE_MS,
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
 *
 * A cache request that never answers is a miss too. An error is what triggers the fallback,
 * and a request queued behind stalled ones raises none: a grid sat blank, every card waiting,
 * with nothing to recover from. So an image still pending [`CACHE_PATIENCE_MS`] after it came
 * into view goes direct as well.
 */
export default function CachedImg({
  src,
  width,
  onUnavailable,
  ...rest
}: CachedImgProps) {
  // `direct` from the start once the scheme has been written off, so the rest of a session
  // doesn't pay a doomed request per image.
  const [direct, setDirect] = useState(() => schemeIsBroken(src));
  // Set when this image is showing *because* of the fallback, so its load can be reported.
  const [recovered, setRecovered] = useState(false);
  const [settled, setSettled] = useState(false);
  const ref = useRef<HTMLImageElement>(null);

  const fallBack = () => {
    setDirect(true);
    setRecovered(true);
  };

  // The clock starts when the image is near the viewport, not at mount: a lazy image below
  // the fold hasn't been requested yet, and must not be sent direct for waiting its turn.
  useEffect(() => {
    const img = ref.current;
    if (direct || settled || !img) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observer = new IntersectionObserver(
      (entries) => {
        if (timer === undefined && entries.some((e) => e.isIntersecting)) {
          timer = setTimeout(() => {
            // Not reported as a recovery: slow is not broken, and three slow images must not
            // send the rest of the session around the cache.
            if (!img.complete) setDirect(true);
          }, CACHE_PATIENCE_MS);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(img);
    return () => {
      observer.disconnect();
      clearTimeout(timer);
    };
  }, [direct, settled]);

  return (
    <img
      {...rest}
      ref={ref}
      // Keyed so React swaps the element rather than reusing one the browser has already
      // marked as errored, which can leave the new `src` unrequested.
      key={direct ? "direct" : "cached"}
      // The direct request must not carry the app's origin as a referer: the Shop's CDN
      // refuses any referer but its own store, and answers a request with none.
      referrerPolicy="no-referrer"
      src={direct ? src : cachedImage(src, width)}
      onError={() => {
        if (direct) {
          onUnavailable?.();
          return;
        }
        fallBack();
      }}
      onLoad={() => {
        setSettled(true);
        if (recovered) {
          setRecovered(false);
          noteDirectRecovery(src);
        }
      }}
    />
  );
}
