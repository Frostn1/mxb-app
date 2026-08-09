import { useEffect, useRef, useState } from "react";
import { textureBytes } from "../../api/mods";

/**
 * A small preview of one texture sheet, drawn from the raw RGBA the backend is holding.
 *
 * Sampled down in JS rather than by the canvas: a 4096² sheet is 67 MB of pixels, and
 * putting all of them into an ImageData just to draw them at 96 px would cost that much
 * again per sheet. Nearest-neighbour is right here for the same reason it would be wrong
 * in the packer — this is a thumbnail, not artwork.
 */
export function SheetThumb({
  token,
  width,
  height,
  size = 96,
  className,
}: {
  token: string;
  width: number;
  height: number;
  size?: number;
  className?: string;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setFailed(false);
    textureBytes(token)
      .then((buf) => {
        const el = canvas.current;
        if (!alive || !el) return;
        const src = new Uint8ClampedArray(buf);
        // An evicted token comes back as a single grey pixel — there's nothing to draw.
        if (src.length !== width * height * 4) {
          setFailed(true);
          return;
        }
        const scale = Math.max(1, Math.ceil(Math.max(width, height) / size));
        const w = Math.max(1, Math.floor(width / scale));
        const h = Math.max(1, Math.floor(height / scale));
        el.width = w;
        el.height = h;
        const ctx = el.getContext("2d");
        if (!ctx) return;
        const out = ctx.createImageData(w, h);
        for (let y = 0; y < h; y++) {
          for (let x = 0; x < w; x++) {
            const s = ((y * scale) * width + x * scale) * 4;
            const d = (y * w + x) * 4;
            out.data[d] = src[s];
            out.data[d + 1] = src[s + 1];
            out.data[d + 2] = src[s + 2];
            out.data[d + 3] = src[s + 3];
          }
        }
        ctx.putImageData(out, 0, 0);
      })
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [token, width, height, size]);

  if (failed) {
    return (
      <div
        className={className}
        style={{ width: size, height: size }}
        aria-hidden
      />
    );
  }
  return (
    <canvas
      ref={canvas}
      className={className}
      style={{ width: size, height: size, imageRendering: "pixelated" }}
    />
  );
}
