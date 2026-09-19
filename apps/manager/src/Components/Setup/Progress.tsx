import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

interface ProgressProps {
  /** How many steps this run of setup has — two on a single-title build, three otherwise. */
  total: number;
  /** Which one is on screen, counting from one. */
  current: number;
}

/**
 * Where you are in setup. Worth the row of pixels because the flow now asks for a Steam
 * sign-in in the middle of it: without a count, a step that opens the browser and waits
 * reads as the app having stopped rather than as one of three things being asked.
 */
export default function Progress({ total, current }: ProgressProps) {
  const t = useT();
  return (
    <div className="flex flex-col items-center gap-2.5">
      <div className="flex items-center gap-1.5">
        {Array.from({ length: total }, (_, i) => (
          <span
            key={i}
            className={cn(
              "h-1.5 rounded-full transition-all",
              i === current - 1
                ? "w-6 bg-primary"
                : i < current - 1
                  ? "w-3 bg-primary/40"
                  : "w-3 bg-foreground/15",
            )}
          />
        ))}
      </div>
      <span className="font-cond text-[11.5px] font-semibold uppercase tracking-[0.06em] text-faint">
        {t("setup.step", { current, total })}
      </span>
    </div>
  );
}
