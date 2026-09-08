import { X } from "lucide-react";
import { Progress } from "@frost/shared/Components/ui/progress";
import { isRunning, PHASE_KEY, useTrackBuild } from "../../../Context/TrackBuild";
import { useT } from "@frost/shared/i18n/context";
import { cn } from "@frost/shared/lib/utils";

/**
 * What the build is doing, under the button that started it.
 *
 * A track is minutes of compiling with nothing on screen to say so, and the old studio said
 * only "Compiling…" on a disabled button for the whole of it. This is the phase it is on and
 * how far along, and afterwards it is the list of what each compiler produced.
 */
export default function BuildCard() {
  const t = useT();
  const { build, dismiss } = useTrackBuild();
  if (!build) return null;

  const live = isRunning(build.state);
  const pct = Math.round(build.progress * 100);

  return (
    <div className="flex flex-col gap-2 rounded-xl border border-input p-3">
      <div className="flex items-baseline justify-between gap-2">
        <span
          className={cn(
            "truncate text-[11.5px] font-semibold",
            build.state === "failed" ? "text-destructive" : "text-foreground/85",
          )}
        >
          {t(PHASE_KEY[build.state])}
        </span>
        {live ? (
          <span className="flex-none tabular-nums text-[10.5px] text-muted-foreground">
            {pct}%
          </span>
        ) : (
          <button
            onClick={dismiss}
            aria-label={t("common.dismiss")}
            className="-mr-1 flex-none cursor-default rounded-full p-0.5 text-muted-foreground transition-colors hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        )}
      </div>

      <Progress
        value={pct}
        className="h-[3px] rounded-full"
        barClassName={cn("rounded-full", build.state === "failed" && "bg-destructive")}
      />

      {/* Only while it runs: once it is over, the step list below says the same thing with
          more in it, and the track's name is on screen anyway. */}
      {live && (
        <span className="text-[10.5px] leading-snug text-muted-foreground">
          {t("track.compileHint")}
        </span>
      )}

      {build.steps.length > 0 && (
        <ul className="space-y-1 text-[11.5px] leading-snug">
          {build.steps.map((s) => (
            <li
              key={s.name}
              className={s.ok ? "text-muted-foreground" : "text-destructive"}
            >
              {s.ok ? "✓" : "✕"} {s.name}
              {s.produced ? ` → ${s.produced}` : ""}
            </li>
          ))}
        </ul>
      )}

      {/* A build that never reached a compiler — no Wine prefix, no tools — has no steps to
          show, so its reason has nowhere else to go. */}
      {build.state === "failed" && build.steps.length === 0 && build.error && (
        <span className="line-clamp-4 text-[11px] leading-snug text-destructive">
          {build.error}
        </span>
      )}
    </div>
  );
}
