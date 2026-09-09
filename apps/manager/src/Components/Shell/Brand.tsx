import type { ReactNode } from "react";
import { cn } from "@frost/shared/lib/utils";

/**
 * The mark's shape on its own, for the screens that show an icon rather than the wordmark
 * — setup, the welcome slides, the tour. It replaces a rounded-square gradient tile, which
 * is the single most recognisable "generated app" tell there is.
 */
export function Plate({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <span className={cn("u-skew grid place-items-center bg-primary text-primary-foreground", className)}>
      <span className="u-unskew grid place-items-center">{children}</span>
    </span>
  );
}

/**
 * The wordmark: a race plate carrying FROST, then the rest of the name.
 *
 * The plate is a skewed box with the label skewed back, matching `logo.svg` — which draws
 * the same shape as paths so the favicon and installer icon do not depend on Barlow
 * Condensed being available.
 */
export default function Brand() {
  return (
    <div data-tauri-drag-region className="flex select-none items-center">
      {/* Both halves are set at the same size: the plate was carrying 12px type next to a
          17px word, which read as two different logos sitting together. */}
      <span className="u-skew grid h-[26px] place-items-center bg-primary px-2">
        <span className="u-unskew font-cond text-[16px] font-bold leading-none tracking-[0.04em] text-primary-foreground">
          FROST
        </span>
      </span>
      {/* Looser than the plate but tighter than the old one-word half: "Mod Manager" is
          eleven characters, and 0.12em on that read as two spaced-out words rather than
          one wordmark. */}
      <span className="ml-[9px] font-cond text-[16px] font-semibold leading-none tracking-[0.08em] text-muted-foreground">
        Mod Manager
      </span>
    </div>
  );
}
