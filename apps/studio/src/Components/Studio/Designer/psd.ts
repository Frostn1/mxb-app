import { readPsd, writePsd, type Layer as PsdLayer, type Psd } from "ag-psd";
import { clampRegion, newId, type BlendMode, type Layer, type Sheet } from "./layers";
import { composite, selectionBounds } from "./composite";

/**
 * Photoshop, in both directions: a `.psd` opened as sheets, and sheets written back out as one.
 *
 * The Designer draws liveries, and almost every livery in circulation started life as a layered
 * Photoshop file. Until now the only way in was to flatten one first — which threw away the
 * layers and made the app a worse editor than the file it was fed — and the only way out was a
 * packed `.pnt`, which no image editor opens. Both ends of that are here.
 *
 * **The sheet is upside down and the PSD is not.** A `.pnt` stores its rows in the order the
 * mesh samples them; the 2D stage turns that over for display, and the PSD is written the way
 * the stage shows it, because that is the way a painter's template is drawn. So every crossing
 * in this file flips, and the flip is the only place the two spaces are allowed to meet — see
 * `CanvasStage` for the same rule stated from the view's side.
 *
 * **A sheet is a document, not a layer group.** Sheets have their own sizes, and a PSD has one
 * canvas, so a paint with a `plastics` and a `number` becomes two files rather than one with
 * two folders in it. Anything else would have to resample one of them to fit the other.
 */

/** Blend modes the two formats agree on. Everything else in a PSD lands as `normal`. */
const FROM_PSD: Record<string, BlendMode> = {
  normal: "normal",
  multiply: "multiply",
  screen: "screen",
  overlay: "overlay",
};

/** Room around a layer's measured bounds, for the antialiasing that lands just outside them. */
const BLEED = 2;

function canvasOf(width: number, height: number): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
}

/**
 * A copy of `src`'s `region` turned top-to-bottom — sheet space in, stage space out.
 *
 * One call does the crop and the flip together, because doing them separately would mean
 * allocating a sheet-sized canvas per layer to hold the intermediate. A bike's `plastics` is
 * 4096² and a paint carries a dozen of them; that difference is measured in gigabytes.
 */
function cropFlipped(
  src: HTMLCanvasElement,
  x: number,
  y: number,
  w: number,
  h: number,
): HTMLCanvasElement {
  const out = canvasOf(w, h);
  const ctx = out.getContext("2d");
  if (!ctx) return out;
  ctx.translate(0, h);
  ctx.scale(1, -1);
  ctx.drawImage(src, x, y, w, h, 0, 0, w, h);
  return out;
}

/**
 * One Designer layer as one PSD layer, or null when none of it lands on the sheet.
 *
 * Rendered through `composite` rather than re-derived, so what Photoshop opens is drawn by the
 * same code as what the game gets — a second implementation of the transform stack is a second
 * chance to disagree with it. Opacity and blending are lifted *out* of the render and handed to
 * Photoshop as layer properties instead: baked in, they would stop being adjustable, which is
 * the whole reason for exporting layers rather than a flat image.
 */
function layerToPsd(sheet: Sheet, layer: Layer, scratch: HTMLCanvasElement): PsdLayer | null {
  const bounds = selectionBounds([layer]);
  if (!bounds) return null;
  const region = clampRegion(
    { x: bounds.x - BLEED, y: bounds.y - BLEED, w: bounds.w + BLEED * 2, h: bounds.h + BLEED * 2 },
    sheet.width,
    sheet.height,
  );
  if (!region) return null;
  composite(scratch, {
    ...sheet,
    base: null,
    layers: [{ ...layer, visible: true, opacity: 1, blend: "normal" }],
  });
  const canvas = cropFlipped(scratch, region.x, region.y, region.w, region.h);
  // The flip, as coordinates: a band `region.y` up from the bottom of the sheet is the same
  // band `height - (y + h)` down from the top of the document.
  const top = sheet.height - (region.y + region.h);
  return {
    name: layer.name,
    top,
    left: region.x,
    bottom: top + region.h,
    right: region.x + region.w,
    opacity: layer.opacity,
    blendMode: layer.blend,
    hidden: !layer.visible,
    canvas,
  };
}

/**
 * A sheet as a Photoshop document, layers intact.
 *
 * `flat` is the sheet already composited — the editor keeps one per sheet and the save path
 * has just drawn it, so this takes it rather than making a second one. It becomes the
 * document's own image: the flattened picture every viewer that isn't Photoshop shows.
 *
 * Groups come across as groups. A Designer group is a tag rather than a container, but
 * `regroup` keeps a tag's members next to each other in the stack, so a run of them is exactly
 * a folder — and one that comes back as a group when the file is opened again here.
 */
export function sheetToPsd(sheet: Sheet, flat: HTMLCanvasElement): ArrayBuffer {
  const scratch = canvasOf(sheet.width, sheet.height);
  const children: PsdLayer[] = [];

  // The template the sheet was opened from, underneath everything, as a layer of its own —
  // it is what the paint contains, so leaving it out would export a livery with no bike
  // under it.
  if (sheet.base) {
    const under = canvasOf(sheet.width, sheet.height);
    const ctx = under.getContext("2d");
    if (ctx) {
      ctx.translate(0, sheet.height);
      ctx.scale(1, -1);
      ctx.drawImage(sheet.base, 0, 0, sheet.width, sheet.height);
    }
    children.push({
      name: sheet.name || "base",
      top: 0,
      left: 0,
      bottom: sheet.height,
      right: sheet.width,
      canvas: under,
    });
  }

  // Bottom-first, which is the order both sides already keep — `Sheet.layers` and a PSD's
  // `children` agree about which end of the array is the back of the stack.
  let i = 0;
  while (i < sheet.layers.length) {
    const tag = sheet.layers[i].group;
    if (!tag) {
      const one = layerToPsd(sheet, sheet.layers[i], scratch);
      if (one) children.push(one);
      i += 1;
      continue;
    }
    const members: PsdLayer[] = [];
    while (i < sheet.layers.length && sheet.layers[i].group === tag) {
      const one = layerToPsd(sheet, sheet.layers[i], scratch);
      if (one) members.push(one);
      i += 1;
    }
    if (members.length) children.push({ name: tag, opened: true, children: members });
  }

  const psd: Psd = {
    width: sheet.width,
    height: sheet.height,
    canvas: cropFlipped(flat, 0, 0, sheet.width, sheet.height),
    children,
  };
  // Trimmed, because every layer above was rendered into a box measured from its transform
  // rather than from its ink — a rotated logo fills a little over half of one.
  return writePsd(psd, { trimImageData: true, generateThumbnail: true });
}

/** A PSD layer's pixels as an `ImageBitmap`, or null for a layer that carries none. */
async function bitmapOf(layer: PsdLayer): Promise<ImageBitmap | null> {
  const data = layer.imageData;
  if (!data || !data.width || !data.height) return null;
  // Copied rather than wrapped: the reader hands back a view into its own buffer, and an
  // `ImageData` over that would keep the whole decoded document alive behind one layer.
  return createImageBitmap(new ImageData(new Uint8ClampedArray(data.data), data.width, data.height));
}

/** Every leaf of a PSD's layer tree, bottom-first, each carrying the folder it came out of. */
function flatten(
  children: PsdLayer[],
  tag: string | null,
  out: { layer: PsdLayer; tag: string | null }[],
) {
  for (const child of children) {
    if (child.children) {
      // One tag per folder, however deep — the Designer's grouping doesn't nest, and a nested
      // folder flattened into its parent's tag still moves as one, which is what a group is for.
      flatten(child.children, tag ?? newId("group"), out);
    } else {
      out.push({ layer: child, tag });
    }
  }
}

/**
 * A `.psd` as a sheet: its layers, in place, still separate.
 *
 * Every layer arrives as an image, type included. A PSD's text is a font, a size and a
 * transform this app has no way to resolve — the family may not be installed, and Photoshop's
 * layout is not the canvas's — so the rasterised pixels Photoshop already stored are the only
 * honest reading of what the file looks like. They are also, importantly, editable *as pixels*
 * here: moved, scaled, masked to a part.
 *
 * The sheet is named after the file, because that is how a `.tga` template names a sheet too,
 * and the name is the entire binding to the model.
 */
export async function psdToSheet(bytes: ArrayBuffer, fileName: string): Promise<Sheet> {
  // `useImageData` rather than canvases: a canvas premultiplies, and a livery's soft edges are
  // exactly the pixels that lose precision on the way through one.
  const psd = readPsd(bytes, {
    useImageData: true,
    skipCompositeImageData: true,
    skipThumbnail: true,
    skipLinkedFilesData: true,
  });
  const flat: { layer: PsdLayer; tag: string | null }[] = [];
  flatten(psd.children ?? [], null, flat);

  const layers: Layer[] = [];
  for (const { layer, tag } of flat) {
    const image = await bitmapOf(layer);
    if (!image) continue;
    const left = layer.left ?? 0;
    const top = layer.top ?? 0;
    layers.push({
      id: newId("layer"),
      kind: "image",
      name: layer.name || fileName,
      visible: !layer.hidden,
      opacity: layer.opacity ?? 1,
      blend: FROM_PSD[layer.blendMode ?? "normal"] ?? "normal",
      x: left + image.width / 2,
      // Back into the sheet's row order. An image layer goes into the composite mirrored
      // (see `mirrored`), so the bitmap itself needs no turning — only where its centre sits.
      y: psd.height - (top + image.height / 2),
      scale: 1,
      rotation: 0,
      flipX: false,
      flipY: false,
      group: tag,
      mirror: null,
      clip: null,
      image,
    });
  }

  return {
    id: newId("sheet"),
    name: fileName,
    width: psd.width,
    height: psd.height,
    base: null,
    layers,
  };
}
