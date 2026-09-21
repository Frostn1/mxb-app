import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

interface ProgressProps {
  /** How many questions this setup run actually needs to ask. */
  total: number;
  /** Which one is on screen, counting from one. */
  current: number;
}

/** Where the player is in the short game, folder and integration setup flow. */
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
