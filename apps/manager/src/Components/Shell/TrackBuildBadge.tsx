import { Hammer, X } from "lucide-react";
import { isRunning, PHASE_KEY, useTrackBuild } from "../../Context/TrackBuild";
import { useT } from "@/i18n";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";
import { Progress } from "@frost/shared/Components/ui/progress";
import { cn } from "@frost/shared/lib/utils";

/**
 * A track compiling, in the rail.
 *
 * The whole point of holding the build above the views is that you can go and do something
 * else while it runs — which is only true if you can still see it from wherever you went.
 * The ring on the icon is the progress; the panel says which track and which phase, and
 * takes you back to the Studio.
 */
export default function TrackBuildBadge({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  const { build, dismiss } = useTrackBuild();

  // A finished build has already said so in a toast. Only a failure stays up, because that
  // one has something to go back to.
  if (!build || build.state === "done") return null;
  const live = isRunning(build.state);
  const pct = Math.round(build.progress * 100);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          title={t("track.buildingTitle", { name: build.name })}
          aria-label={t("track.buildingTitle", { name: build.name })}
          className="relative flex cursor-default items-center justify-center rounded-lg px-1.5 py-2.5 text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
        >
          <Hammer className={cn("size-4", !live && "text-destructive")} />
          {live && (
            <span className="absolute inset-x-1 bottom-1 h-[2px] overflow-hidden rounded-full bg-foreground/[0.14]">
              <span
                className="block h-full rounded-full bg-primary transition-[width] duration-300"
                style={{ width: `${pct}%` }}
              />
            </span>
          )}
        </button>
      </PopoverTrigger>

      <PopoverContent align="end" className="w-[280px] p-0">
        <div className="flex items-center justify-between gap-2 border-b border-white/[0.07] px-3.5 py-2.5">
          <span className="truncate text-[12px] font-bold">
            {t("track.buildingTitle", { name: build.name })}
          </span>
          {!live && (
            <button
              onClick={dismiss}
              aria-label={t("common.dismiss")}
              className="-mr-1 flex-none cursor-default rounded-full p-0.5 text-muted-foreground transition-colors hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          )}
        </div>
        <div className="flex flex-col gap-2 px-3.5 py-3">
          <div className="flex items-baseline justify-between gap-2">
            <span
              className={cn(
                "truncate text-[11.5px]",
                live ? "text-muted-foreground" : "text-destructive",
              )}
            >
              {t(PHASE_KEY[build.state])}
            </span>
            {live && (
              <span className="flex-none tabular-nums text-[10.5px] text-muted-foreground">
                {pct}%
              </span>
            )}
          </div>
          <Progress
            value={pct}
            className="h-[3px] rounded-full"
            barClassName={cn("rounded-full", !live && "bg-destructive")}
          />
          <button
            onClick={onOpen}
            className="mt-0.5 cursor-default text-left text-[11px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
          >
            {t("track.openStudio")}
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
