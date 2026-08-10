/**
 * What a paint being drawn is made of.
 *
 * A `.pnt` is a set of named sheets, and the name is the part that matters — the mesh binds
 * `livery`, not "the first one". So a `Sheet` here is a texture name with pixels attached,
 * and everything the editor does is arranged under that: layers stack onto a sheet, sheets
 * make a paint, and the names survive to the save.
 *
 * Layers hold decoded pixels (`ImageBitmap`) rather than paths. Compositing happens on every
 * pointer move, and a round trip to the backend per frame would make dragging a logo feel
 * like dragging through treacle.
 */

/** Canvas composite operations, under names a painter would recognise. */
export type BlendMode = "normal" | "multiply" | "screen" | "overlay";

export const BLEND_MODES: BlendMode[] = ["normal", "multiply", "screen", "overlay"];

export const BLEND_OPS: Record<BlendMode, GlobalCompositeOperation> = {
  normal: "source-over",
  multiply: "multiply",
  screen: "screen",
  overlay: "overlay",
};

interface LayerCommon {
  id: string;
  name: string;
  visible: boolean;
  /** 0–1. */
  opacity: number;
  blend: BlendMode;
  /** Centre of the layer, in the sheet's own pixels. */
  x: number;
  y: number;
  scale: number;
  /** Radians, clockwise. */
  rotation: number;
}

export interface ImageLayer extends LayerCommon {
  kind: "image";
  image: ImageBitmap;
}

export interface TextLayer extends LayerCommon {
  kind: "text";
  text: string;
  /** A family the webview can resolve — see `FONTS`. */
  font: string;
  /** Cap height in sheet pixels, before `scale`. */
  size: number;
  color: string;
  outline: string;
  /** 0 turns the outline off. */
  outlineWidth: number;
}

export type Layer = ImageLayer | TextLayer;

/** One texture of the paint: a name the mesh binds, and what's drawn under that name. */
export interface Sheet {
  id: string;
  /** The texture name. Renameable, and the single thing that decides where it lands. */
  name: string;
  width: number;
  height: number;
  /** What the sheet started as — an unpacked template, or nothing for a blank one. */
  base: ImageBitmap | null;
  /** Bottom-first, so the array order is the stacking order. */
  layers: Layer[];
}

/**
 * Families to offer for text.
 *
 * Deliberately generic: these resolve against whatever the OS has, and a paint drawn with a
 * font the machine lacks would silently fall back — on the sheet, permanently, because the
 * text is rasterised into the saved pixels. Sticking to families every platform resolves
 * keeps what you saw the thing that got saved.
 */
export const FONTS = [
  "Arial, Helvetica, sans-serif",
  "Impact, Haettenschweiler, sans-serif",
  "Georgia, 'Times New Roman', serif",
  "'Courier New', monospace",
  "Verdana, Geneva, sans-serif",
];

let seq = 0;
export function newId(prefix: string): string {
  seq += 1;
  return `${prefix}-${seq}`;
}

/** A measuring context. Text extents need one, and this never draws anything. */
let measureCtx: CanvasRenderingContext2D | null = null;
function measurer(): CanvasRenderingContext2D | null {
  if (!measureCtx) {
    measureCtx = document.createElement("canvas").getContext("2d");
  }
  return measureCtx;
}

export function fontSpec(layer: TextLayer): string {
  return `bold ${layer.size}px ${layer.font}`;
}

/**
 * A layer's unrotated, unscaled extent in sheet pixels.
 *
 * Text is measured rather than guessed — `actualBoundingBox*` gives the inked extent, which is
 * what a selection box should hug. Falls back to a rough estimate if the context is missing,
 * which only matters for hit-testing accuracy, never for what gets drawn.
 */
export function layerExtent(layer: Layer): { w: number; h: number } {
  if (layer.kind === "image") {
    return { w: layer.image.width, h: layer.image.height };
  }
  const ctx = measurer();
  if (!ctx) return { w: layer.text.length * layer.size * 0.6, h: layer.size };
  ctx.font = fontSpec(layer);
  const m = ctx.measureText(layer.text || " ");
  const h =
    m.actualBoundingBoxAscent + m.actualBoundingBoxDescent || layer.size;
  return { w: Math.max(m.width, 1) + layer.outlineWidth * 2, h: h + layer.outlineWidth * 2 };
}

/** Half-extents after the layer's own scale — the box a pointer is tested against. */
export function layerHalfSize(layer: Layer): { hw: number; hh: number } {
  const { w, h } = layerExtent(layer);
  return { hw: (w * layer.scale) / 2, hh: (h * layer.scale) / 2 };
}

/**
 * The topmost visible layer under a point in sheet coordinates, or null.
 *
 * Walks back to front because that's the one a click means: the layer you can see.
 */
export function hitTest(layers: Layer[], px: number, py: number): Layer | null {
  for (let i = layers.length - 1; i >= 0; i -= 1) {
    const layer = layers[i];
    if (!layer.visible) continue;
    // Into the layer's own frame: translate to its centre, then undo its rotation.
    const dx = px - layer.x;
    const dy = py - layer.y;
    const cos = Math.cos(-layer.rotation);
    const sin = Math.sin(-layer.rotation);
    const lx = dx * cos - dy * sin;
    const ly = dx * sin + dy * cos;
    const { hw, hh } = layerHalfSize(layer);
    if (Math.abs(lx) <= hw && Math.abs(ly) <= hh) return layer;
  }
  return null;
}

export function imageLayer(name: string, image: ImageBitmap, sheet: Sheet): ImageLayer {
  // Land it centred, and shrunk to fit if it's bigger than the sheet — a 2048² logo dropped
  // onto a 2048² livery would otherwise cover the whole thing with no visible handle to grab.
  const fit = Math.min(1, (sheet.width * 0.6) / image.width, (sheet.height * 0.6) / image.height);
  return {
    kind: "image",
    id: newId("layer"),
    name,
    visible: true,
    opacity: 1,
    blend: "normal",
    x: sheet.width / 2,
    y: sheet.height / 2,
    scale: fit,
    rotation: 0,
    image,
  };
}

export function textLayer(text: string, sheet: Sheet): TextLayer {
  return {
    kind: "text",
    id: newId("layer"),
    name: text,
    visible: true,
    opacity: 1,
    blend: "normal",
    x: sheet.width / 2,
    y: sheet.height / 2,
    scale: 1,
    rotation: 0,
    text,
    font: FONTS[0],
    // A twelfth of the sheet reads as a name across a jersey back at 2048².
    size: Math.round(sheet.height / 12),
    color: "#ffffff",
    outline: "#000000",
    outlineWidth: 0,
  };
}

export function blankSheet(name: string, size: number): Sheet {
  return { id: newId("sheet"), name, width: size, height: size, base: null, layers: [] };
}
