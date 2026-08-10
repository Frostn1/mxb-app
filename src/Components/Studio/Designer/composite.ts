import * as THREE from "three";
import {
  BLEND_OPS,
  fontSpec,
  layerExtent,
  type Layer,
  type Sheet,
} from "./layers";

/**
 * Drawing a sheet, and getting the result out as something the rest of the app can use.
 *
 * There is exactly one composite per sheet and everything reads from it: the 2D view draws it
 * scaled, the 3D preview wraps it in a texture, and the save encodes it. Compositing twice —
 * once for the screen and once for the file — is how a preview ends up lying about what was
 * saved, so it isn't done.
 */

/** Draw one layer into a context already sized to the sheet. */
function drawLayer(ctx: CanvasRenderingContext2D, layer: Layer) {
  if (!layer.visible || layer.opacity <= 0) return;
  ctx.save();
  ctx.globalAlpha = layer.opacity;
  ctx.globalCompositeOperation = BLEND_OPS[layer.blend];
  ctx.translate(layer.x, layer.y);
  ctx.rotate(layer.rotation);
  ctx.scale(layer.scale, layer.scale);

  if (layer.kind === "image") {
    ctx.drawImage(layer.image, -layer.image.width / 2, -layer.image.height / 2);
  } else {
    ctx.font = fontSpec(layer);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.lineJoin = "round";
    // Stroke first so the outline sits behind the fill rather than eating half of it.
    if (layer.outlineWidth > 0) {
      ctx.lineWidth = layer.outlineWidth * 2;
      ctx.strokeStyle = layer.outline;
      ctx.strokeText(layer.text, 0, 0);
    }
    ctx.fillStyle = layer.color;
    ctx.fillText(layer.text, 0, 0);
  }
  ctx.restore();
}

/**
 * Redraw `sheet` into `canvas`, sizing it if needed.
 *
 * Cleared to transparent, not to white: a paint's alpha is the part of it the game reads for
 * decals and cutouts, and a sheet flattened onto white would lose that on the way to the file.
 */
export function composite(canvas: HTMLCanvasElement, sheet: Sheet): void {
  if (canvas.width !== sheet.width || canvas.height !== sheet.height) {
    canvas.width = sheet.width;
    canvas.height = sheet.height;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = "source-over";
  ctx.clearRect(0, 0, sheet.width, sheet.height);
  if (sheet.base) ctx.drawImage(sheet.base, 0, 0, sheet.width, sheet.height);
  for (const layer of sheet.layers) drawLayer(ctx, layer);
}

/** The selection box for a layer, as the four corners of its rotated bounds in sheet space. */
export function layerCorners(layer: Layer): [number, number][] {
  const { w, h } = layerExtent(layer);
  const hw = (w * layer.scale) / 2;
  const hh = (h * layer.scale) / 2;
  const cos = Math.cos(layer.rotation);
  const sin = Math.sin(layer.rotation);
  return (
    [
      [-hw, -hh],
      [hw, -hh],
      [hw, hh],
      [-hw, hh],
    ] as [number, number][]
  ).map(([x, y]) => [layer.x + x * cos - y * sin, layer.y + x * sin + y * cos]);
}

/**
 * The sheet as PNG bytes, for staging before a save.
 *
 * The rows leave exactly as they were drawn, which is exactly as they were read — the editor
 * takes no view on which way up a sheet is, because the format doesn't either.
 *
 * PNG because it's lossless and what a canvas encodes natively — the sheet is decoded again
 * on the Rust side on its way into the `.pnt`, so the intermediate format only has to not
 * lose anything.
 */
export function toPng(canvas: HTMLCanvasElement): Promise<ArrayBuffer> {
  return new Promise<ArrayBuffer>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        reject(new Error("the sheet could not be encoded"));
        return;
      }
      blob.arrayBuffer().then(resolve, reject);
    }, "image/png");
  });
}

/**
 * The sheet as a texture the viewer will map exactly like an installed paint.
 *
 * A `DataTexture` built from the canvas's own pixels, with the settings `loadTexture` uses —
 * not a `CanvasTexture`. The two upload differently (`flipY` applies to a canvas and not to a
 * raw array), so a `CanvasTexture` put the drawing on the model the other way up from the
 * `.pnt` it came from. Matching the type is the only way to be sure they agree; reasoning
 * about which `flipY` cancels which got it wrong twice.
 *
 * The cost is a full-size readback per change, which is the same order as the composite that
 * just ran.
 */
export function sheetTexture(
  canvas: HTMLCanvasElement,
  existing: THREE.DataTexture | null,
): THREE.DataTexture | null {
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx || !canvas.width || !canvas.height) return existing;
  const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);

  // Same size as last time: write into the buffer that's already there rather than handing
  // three.js a new one, so a drag doesn't allocate a sheet-sized array per frame.
  const held = existing?.image.data as Uint8Array | undefined;
  if (existing && held && held.length === data.length) {
    held.set(data);
    existing.needsUpdate = true;
    return existing;
  }

  existing?.dispose();
  const pixels = new Uint8Array(data.buffer.slice(0));
  const tex = new THREE.DataTexture(pixels, canvas.width, canvas.height, THREE.RGBAFormat);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.flipY = false;
  tex.wrapS = THREE.RepeatWrapping;
  tex.wrapT = THREE.RepeatWrapping;
  tex.magFilter = THREE.LinearFilter;
  tex.minFilter = THREE.LinearMipmapLinearFilter;
  tex.generateMipmaps = true;
  tex.anisotropy = 4;
  tex.needsUpdate = true;
  return tex;
}

/** Decode raw RGBA — the shape everything crosses the IPC boundary in — into a bitmap. */
export function bitmapFromRgba(
  buf: ArrayBuffer,
  width: number,
  height: number,
): Promise<ImageBitmap> {
  const expected = width * height * 4;
  if (buf.byteLength !== expected) {
    // The texture store evicted it, or something is badly out of step. Say which, because
    // the alternative is `ImageData` throwing a DOM error that names neither.
    return Promise.reject(
      new Error(`expected ${expected} bytes for ${width}×${height}, got ${buf.byteLength}`),
    );
  }
  return createImageBitmap(new ImageData(new Uint8ClampedArray(buf), width, height));
}
