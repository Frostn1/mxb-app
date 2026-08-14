import * as THREE from "three";
import {
  BLEND_OPS,
  clampRegion,
  fontSpec,
  layerExtent,
  type Layer,
  type Region,
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
  // Before the layer's own transform, because the path is in sheet pixels: a shroud stays where
  // the mesh put it however the photo inside it is dragged, scaled or spun. Clipping after the
  // translate would carry the part around with the layer, which is the opposite of pinning it.
  if (layer.clip) ctx.clip(layer.clip.path, "nonzero");
  ctx.translate(layer.x, layer.y);
  ctx.rotate(layer.rotation);
  ctx.scale(layer.scale, layer.scale);

  if (layer.kind === "image" || layer.kind === "paint") {
    // A paint layer's raster is sheet-sized and its transform is the identity, so this puts it
    // back exactly where it was painted — the same call, from the same centre, as an image.
    const src = layer.kind === "image" ? layer.image : layer.canvas;
    ctx.drawImage(src, -src.width / 2, -src.height / 2);
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
 * Redraw `sheet` into `canvas`, sizing it if needed, and say which part of it was rewritten.
 *
 * Cleared to transparent, not to white: a paint's alpha is the part of it the game reads for
 * decals and cutouts, and a sheet flattened onto white would lose that on the way to the file.
 *
 * `region` narrows the redraw to the part of the sheet the caller knows has moved — a brush
 * stamp rather than the 2048² square around it. Every layer is still drawn, under a clip: the
 * saving is in pixels touched, not in work skipped, and a stack that composited differently
 * inside the region than outside it would leave a seam along the region's edge. Anything that
 * cannot be narrowed honestly — a sheet that just changed size, or a change nobody described —
 * passes null and gets the whole thing.
 *
 * The region that comes back is the one actually used, clipped to the canvas: it is what the
 * texture readback downstream has to be told, and letting the caller assume its own region was
 * honoured is how a resized sheet would upload one stamp's worth of a new canvas.
 */
export function composite(
  canvas: HTMLCanvasElement,
  sheet: Sheet,
  region?: Region | null,
): Region | null {
  const resized = canvas.width !== sheet.width || canvas.height !== sheet.height;
  if (resized) {
    canvas.width = sheet.width;
    canvas.height = sheet.height;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  // A fresh canvas has nothing on it to keep, so a region would be describing pixels that were
  // never drawn in the first place.
  const area =
    resized || !region
      ? { x: 0, y: 0, w: sheet.width, h: sheet.height }
      : clampRegion(region, sheet.width, sheet.height);
  if (!area || !area.w || !area.h) return null;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.save();
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = "source-over";
  ctx.beginPath();
  ctx.rect(area.x, area.y, area.w, area.h);
  ctx.clip();
  ctx.clearRect(area.x, area.y, area.w, area.h);
  if (sheet.base) ctx.drawImage(sheet.base, 0, 0, sheet.width, sheet.height);
  for (const layer of sheet.layers) drawLayer(ctx, layer);
  ctx.restore();
  return area;
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
/**
 * Whether a composited sheet has a single pixel on it.
 *
 * A `.pnt` replaces the model's textures *by name*, so an untouched sheet is not a harmless
 * blank — saved, it wipes whatever the bike already had under that name. That bites hardest
 * on the companion maps: a transparent `plastics_n` replaces the real normal map and flattens
 * the lighting on the part.
 *
 * The pixels rather than the sheet's layer list, because picking up the brush *creates* a
 * paint layer whether or not a stroke follows, so "has layers" and "has anything on it" are
 * different questions. Reads the alpha channel and stops at the first mark, which is the
 * common case; a genuinely empty 2048² sheet is the one that scans in full.
 */
export function hasInk(canvas: HTMLCanvasElement): boolean {
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return true; // can't tell — saving too much beats silently dropping work
  const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
  for (let i = 3; i < data.length; i += 4) {
    if (data[i] !== 0) return true;
  }
  return false;
}

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
 * `region` is the part of the canvas that changed — the same region the composite just drew.
 * Reading back only those rows is what keeps this off the critical path of a stroke: a full
 * 2048² readback is sixteen megabytes pulled off the GPU and copied again, per pointer sample,
 * to carry a brush stamp that covers a few thousand pixels. The upload is still the whole
 * texture, but that is the GPU's own copy of a buffer it already has; the readback is the
 * expensive half and it is the half a region can remove.
 */
export function sheetTexture(
  canvas: HTMLCanvasElement,
  existing: THREE.DataTexture | null,
  region?: Region | null,
): THREE.DataTexture | null {
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx || !canvas.width || !canvas.height) return existing;

  // Same size as last time: write into the buffer that's already there rather than handing
  // three.js a new one, so a drag doesn't allocate a sheet-sized array per frame.
  const held = existing?.image.data as Uint8Array | undefined;
  if (existing && held && held.length === canvas.width * canvas.height * 4) {
    const area = region ? clampRegion(region, canvas.width, canvas.height) : null;
    if (region && !area) return existing;
    const { x, y, w, h } = area ?? { x: 0, y: 0, w: canvas.width, h: canvas.height };
    const { data } = ctx.getImageData(x, y, w, h);
    if (w === canvas.width) {
      // Full-width rows are contiguous in both buffers, so they go across in one move.
      held.set(data, y * canvas.width * 4);
    } else {
      const stride = canvas.width * 4;
      const run = w * 4;
      for (let row = 0; row < h; row += 1) {
        held.set(data.subarray(row * run, row * run + run), (y + row) * stride + x * 4);
      }
    }
    existing.needsUpdate = true;
    return existing;
  }

  // No buffer to patch — a first texture, or a sheet that changed size. Read the lot.
  const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
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
