import type { ImgHTMLAttributes } from "react";
import CachedImg from "@frost/shared/Components/ui/cached-img";

interface SmartImgProps extends Omit<ImgHTMLAttributes<HTMLImageElement>, "src" | "width"> {
  src: string;
  /** Width to ask the on-disk cache for. Ignored for a `data:` picture. */
  width?: number;
}

/**
 * One picture, whichever of the two kinds it is.
 *
 * Browse and the stores hand out `https://` URLs, which go through the thumbnail cache.
 * The library hands out `data:image/png;base64,…` read straight out of the archive, and
 * putting that through the cache would round-trip a picture we already hold. One component
 * so the page never has to know which source it is drawing.
 */
export default function SmartImg({ src, width, ...rest }: SmartImgProps) {
  if (src.startsWith("data:") || src.startsWith("blob:")) {
    return <img src={src} {...rest} />;
  }
  return <CachedImg src={src} width={width} {...rest} />;
}
