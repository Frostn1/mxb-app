import type { ReactNode } from "react";
import { cn } from "@frost/shared/lib/utils";

/**
 * The mark on its own, for the screens that show an icon rather than the wordmark
 * — setup, the welcome slides, the tour. It matches `logo.svg`: the brand's
 * near-black rounded square, which is also the favicon and the installer icon.
 */
export function Plate({ className, children }: { className?: string; children?: ReactNode }) {
  return (
    <span
      className={cn(
        "grid place-items-center rounded-[22%] bg-[#0b0b0c] text-white ring-1 ring-white/10",
        className,
      )}
    >
      {children ?? (
        <span className="font-cond text-[58%] font-extrabold leading-none tracking-[-0.06em]">m</span>
      )}
    </span>
  );
}

/**
 * The wordmark: the product, then who releases it.
 *
 * mxbsecure is the brand every product ships under, so it appears as a byline rather than
 * as the name — the thing you are looking at is the MXB App. Both halves are the brand's
 * mono; the byline sits on the same baseline so the rail keeps its height.
 */
export default function Brand() {
  return (
    <div data-tauri-drag-region className="flex select-none items-baseline gap-2">
      <span className="font-cond text-[16px] font-extrabold leading-none tracking-[-0.06em]">
        MXB App
      </span>
      <span className="font-cond text-[11px] font-medium leading-none tracking-[-0.02em] text-faint">
        by <span className="font-semibold text-muted-foreground">mxbsecure</span>
      </span>
    </div>
  );
}
