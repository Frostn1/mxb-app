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
