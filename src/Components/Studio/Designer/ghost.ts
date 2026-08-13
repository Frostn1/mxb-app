/**
 * The reference underlay: what a sheet is being drawn *against*, as opposed to what it holds.
 *
 * A livery is drawn flat and worn curved, and the flat version gives away almost nothing about
 * which rectangle of it ends up on a shroud. There are two ways to answer that, and this holds
 * both: the paint you started from, and the model's own UV islands.
 *
 * None of it is part of the sheet. It is never composited, never staged and never saved —
 * `Sheet` is what the `.pnt` will contain, and mixing a guide into it is how a guide ends up
 * shipped inside somebody's livery. The 2D stage draws these separately, which makes that
 * impossible rather than merely unlikely.
 */
export interface Ghost {
  /**
   * The donor paint's pixels, lifted out of `Sheet.base`.
   *
   * Moving it here rather than copying it is the point: a template that is both the ghost and
   * the base would be traced over *and* saved, which is exactly the thing you were trying not
   * to do when you asked for a tracing guide.
   */
  template: ImageBitmap | null;
  /** The UV islands for this sheet's texture name, rasterised once by {@link uvWireframe}. */
  wire: HTMLCanvasElement | null;
  /**
   * The sheet name `wire` was built for, or null if it has never been built.
   *
   * The name is the whole binding — rename the sheet and the islands it maps to are different
   * ones — so this is what says whether the cached raster still describes the sheet.
   */
  wireFor: string | null;
  showTemplate: boolean;
  showWire: boolean;
  /** 0–1, applied to both. */
  opacity: number;
}

export const EMPTY_GHOST: Ghost = {
  template: null,
  wire: null,
  wireFor: null,
  // On by default so lifting a template into the ghost shows it immediately; the UV map is
  // off because it has to be built, and building one for a sheet nobody asked about is work
  // spent on a guide that was never going to be looked at.
  showTemplate: true,
  showWire: false,
  opacity: 0.35,
};

/** True when there is anything to draw — the stage skips the whole pass otherwise. */
export function ghostShows(ghost: Ghost | null | undefined): boolean {
  if (!ghost || ghost.opacity <= 0) return false;
  return (ghost.showTemplate && !!ghost.template) || (ghost.showWire && !!ghost.wire);
}
