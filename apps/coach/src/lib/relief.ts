/** A height grid in world metres: corner at `x0`, `z0`, rows along z, null where unknown. */
export interface Relief {
  x0: number;
  z0: number;
  cell: number;
  width: number;
  height: number;
  heights: ArrayLike<number | null>;
}

/** Relief exaggeration: the slopes on a motocross track are gentle from above. */
const LIFT = 3;

/**
 * The ground as a grey hillshade, lit from the north-west, as a picture the map can lay under
 * the lines. North is up: the canvas's top row is the grid's last (largest z).
 */
export function reliefImage(s: Relief): string {
  const canvas = document.createElement("canvas");
  canvas.width = s.width;
  canvas.height = s.height;
  const g = canvas.getContext("2d");
  if (!g) return "";
  const img = g.createImageData(s.width, s.height);
  const at = (c: number, r: number) =>
    c < 0 || r < 0 || c >= s.width || r >= s.height ? null : s.heights[r * s.width + c];
  let [lo, hi] = [Infinity, -Infinity];
  for (let k = 0; k < s.heights.length; k++) {
    const h = s.heights[k];
    if (h != null) [lo, hi] = [Math.min(lo, h), Math.max(hi, h)];
  }
  const span = Math.max(hi - lo, 0.5);
  const [lx, ly, lz] = [-0.5, 0.75, 0.43];

  for (let r = 0; r < s.height; r++) {
    for (let c = 0; c < s.width; c++) {
      const h = at(c, r);
      if (h == null) continue;
      const dx = ((at(c + 1, r) ?? h) - (at(c - 1, r) ?? h)) / (2 * s.cell);
      const dz = ((at(c, r + 1) ?? h) - (at(c, r - 1) ?? h)) / (2 * s.cell);
      const [nx, ny, nz] = [-dx * LIFT, 1, -dz * LIFT];
      const len = Math.hypot(nx, ny, nz);
      const shade = Math.max(0, (nx * lx + ny * ly + nz * lz) / len);
      const tone = Math.round(34 + 120 * shade + 40 * ((h - lo) / span));
      const k = ((s.height - 1 - r) * s.width + c) * 4;
      img.data[k] = tone;
      img.data[k + 1] = tone;
      img.data[k + 2] = Math.round(tone * 1.04);
      img.data[k + 3] = 255;
    }
  }
  g.putImageData(img, 0, 0);
  return canvas.toDataURL();
}
