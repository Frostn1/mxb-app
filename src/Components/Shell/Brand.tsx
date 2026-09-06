/**
 * The wordmark: a race plate carrying MXB, then APP.
 *
 * The plate is a skewed box with the label skewed back, matching `logo.svg` — which draws
 * the same shape as paths so the favicon and installer icon do not depend on Barlow
 * Condensed being available.
 */
export default function Brand() {
  return (
    <div data-tauri-drag-region className="flex select-none items-center">
      <span className="u-skew grid h-7 w-[34px] place-items-center bg-primary">
        <span className="u-unskew font-cond text-[12px] font-bold tracking-[0.02em] text-primary-foreground">
          MXB
        </span>
      </span>
      <span className="ml-[9px] font-cond text-[17px] font-semibold uppercase tracking-[0.2em] text-muted-foreground">
        App
      </span>
    </div>
  );
}
