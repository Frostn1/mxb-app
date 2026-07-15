import * as THREE from "three";

/**
 * A "livery" is what gets painted onto the bike's bodywork. Today these are
 * generated procedurally (base + accent + number plate) so the Locker is fully
 * self-contained and offline. The pipeline is deliberately texture-based: once
 * the `.pnt` decoder lands, a decoded livery image drops into {@link Livery.texture}
 * via {@link liveryFromImage} with zero changes to the viewer.
 */
export interface Livery {
  id: string;
  name: string;
  /** Primary bodywork colour (also the fallback when no texture is used). */
  base: string;
  /** Accent / stripe colour. */
  accent: string;
  /** Number-plate colour. */
  plate: string;
  /** Number-plate text colour. */
  plateInk: string;
}

/** OEM-flavoured presets so the viewer is instantly populated and recognisable. */
export const PRESET_LIVERIES: Livery[] = [
  { id: "ktm", name: "KTM Factory", base: "#f26522", accent: "#1b1b1f", plate: "#f26522", plateInk: "#111114" },
  { id: "yamaha", name: "Yamaha Racing", base: "#0a3aa4", accent: "#101014", plate: "#ffffff", plateInk: "#0a3aa4" },
  { id: "honda", name: "Honda HRC", base: "#e21e26", accent: "#ffffff", plate: "#ffffff", plateInk: "#e21e26" },
  { id: "kawasaki", name: "Kawasaki", base: "#0f9d58", accent: "#101014", plate: "#101014", plateInk: "#0f9d58" },
  { id: "husqvarna", name: "Husqvarna", base: "#f5f6f8", accent: "#1f3b73", plate: "#f5f6f8", plateInk: "#1f3b73" },
  { id: "gasgas", name: "GasGas", base: "#e2231a", accent: "#1b1b1f", plate: "#ffffff", plateInk: "#e2231a" },
  { id: "carbon", name: "Carbon Fantasy", base: "#17181c", accent: "#6ee7ff", plate: "#17181c", plateInk: "#6ee7ff" },
];

/**
 * Paint a livery onto a 1024² canvas → THREE texture. Draws a base coat, a
 * diagonal accent sweep, a carbon-ish speckle, and a number plate, so the
 * bodywork reads as painted (not a flat colour) under studio lighting.
 */
export function makeLiveryTexture(livery: Livery, number = 21): THREE.Texture {
  const size = 1024;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d")!;

  // Base coat with a subtle vertical sheen.
  const grad = ctx.createLinearGradient(0, 0, 0, size);
  grad.addColorStop(0, shade(livery.base, 1.12));
  grad.addColorStop(0.5, livery.base);
  grad.addColorStop(1, shade(livery.base, 0.82));
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, size, size);

  // Diagonal accent sweeps.
  ctx.save();
  ctx.translate(size / 2, size / 2);
  ctx.rotate((-22 * Math.PI) / 180);
  ctx.fillStyle = livery.accent;
  ctx.globalAlpha = 0.92;
  ctx.fillRect(-size, -size * 0.12, size * 2, size * 0.16);
  ctx.globalAlpha = 0.5;
  ctx.fillRect(-size, size * 0.1, size * 2, size * 0.06);
  ctx.restore();
  ctx.globalAlpha = 1;

  // Fine speckle for a painted-metal grain.
  ctx.globalAlpha = 0.05;
  for (let i = 0; i < 2600; i++) {
    ctx.fillStyle = i % 2 ? "#ffffff" : "#000000";
    ctx.fillRect(Math.random() * size, Math.random() * size, 2, 2);
  }
  ctx.globalAlpha = 1;

  // Number plate, centred.
  const pw = size * 0.42;
  const ph = size * 0.34;
  const px = (size - pw) / 2;
  const py = (size - ph) / 2;
  roundRect(ctx, px, py, pw, ph, 26);
  ctx.fillStyle = livery.plate;
  ctx.fill();
  ctx.lineWidth = 8;
  ctx.strokeStyle = shade(livery.plate, 0.7);
  ctx.stroke();

  ctx.fillStyle = livery.plateInk;
  ctx.font = `900 ${ph * 0.7}px Arial, sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(String(number), size / 2, size / 2 + ph * 0.02);

  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 8;
  tex.needsUpdate = true;
  return tex;
}

/**
 * Build a Livery-shaped texture from an already-decoded image (e.g. a `.pnt`
 * texture the Rust decoder will emit, or a raw `.tga`/`.png` template). This is
 * the seam the real-livery pipeline plugs into later.
 */
export function liveryFromImage(image: HTMLImageElement | HTMLCanvasElement): THREE.Texture {
  const tex = new THREE.CanvasTexture(
    image instanceof HTMLCanvasElement ? image : toCanvas(image),
  );
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 8;
  tex.needsUpdate = true;
  return tex;
}

function toCanvas(img: HTMLImageElement): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = img.naturalWidth || 1024;
  c.height = img.naturalHeight || 1024;
  c.getContext("2d")!.drawImage(img, 0, 0);
  return c;
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** Lighten (>1) or darken (<1) a hex colour. */
function shade(hex: string, factor: number): string {
  const c = new THREE.Color(hex);
  c.multiplyScalar(factor);
  c.r = Math.min(1, c.r);
  c.g = Math.min(1, c.g);
  c.b = Math.min(1, c.b);
  return `#${c.getHexString()}`;
}
