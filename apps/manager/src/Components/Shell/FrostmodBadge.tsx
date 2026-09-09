import { Play, RefreshCw, Square, Zap } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { useConfig } from "@frost/shared/Context/Config";
import { useFrostmod } from "../../Context/FrostmodContext";
import { useT } from "@/i18n";
import { ATTACH_PROBLEM } from "@frost/shared/types";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";

/**
 * FrostMod's state, and the way to start or stop it, in the rail.
 *
 * The old sidebar carried this as a pill on every screen, and the rail that replaced it
 * didn't take it along — so the only Start/Stop left was four screens deep in Settings, and
 * the tour step that points at `[data-tour="frostmod"]` had nothing to point at.
 *
 * It has to be visible from everywhere for the same reason the track build badge does: what
 * it reports changes while you are somewhere else. A dot alone would be a status light, so
 * the panel behind it carries the two actions the sidebar had — reload the game, and stop.
 */
export default function FrostmodBadge() {
  const t = useT();
  const { game } = useConfig();
  const { running, attachment, reload, status, start, stop } = useFrostmod();

  // FrostMod is a compiled MX Bikes plugin — there is nothing to report, start or reload
  // for a title it wasn't built for.
  if (!game.caps.frostmod) return null;

  // Up, but not reaching the game — see `frostmod::attachment`. Reported plainly rather than
  // as "Running", which is exactly as far as a player could get in working out why nothing
  // was happening in game.
  const attachProblem =
    attachment !== null && ATTACH_PROBLEM.includes(attachment.state);

  const label = attachProblem
    ? t("frostmod.notInGame")
    : running === null
      ? t("frostmod.checking")
      : running
        ? t("frostmod.running")
        : t("frostmod.notRunning");

  const onReload = async () => {
    const outcome = await reload();
    if (outcome === "signaled") toast.success(t("frostmod.reloadedGame"));
    else if (outcome === "not_running") toast.info(t("frostmod.notRunningToast"));
  };

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          data-tour="frostmod"
          title={attachProblem ? (attachment?.reason ?? label) : label}
          aria-label={label}
          className="relative flex cursor-default items-center justify-center rounded-lg px-1.5 py-2.5 text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
        >
          <Zap className="size-4" />
          <span
            className={cn(
              "absolute bottom-1.5 right-1 size-[6px] rounded-full ring-2 ring-window",
              running === null
                ? "bg-muted-foreground"
                : attachProblem
                  ? "bg-warning"
                  : running
                    ? "bg-success"
                    : "bg-muted-foreground/50",
            )}
          />
        </button>
      </PopoverTrigger>

      <PopoverContent align="end" className="w-[248px] p-0">
        <div className="flex items-center gap-2 border-b border-white/[0.07] px-3.5 py-2.5">
          <span
            className={cn(
              "size-[7px] flex-none rounded-full",
              running === null
                ? "bg-muted-foreground"
                : attachProblem
                  ? "bg-warning"
                  : running
                    ? "bg-success"
                    : "bg-muted-foreground/50",
            )}
          />
          <span className="truncate text-[12px] font-bold">{label}</span>
        </div>

        {attachProblem && attachment?.reason && (
          <p className="border-b border-white/[0.07] px-3.5 py-2 text-[11px] text-muted-foreground">
            {attachment.reason}
          </p>
        )}

        <div className="flex flex-col gap-1 p-2">
          {running ? (
            <>
              <button
                onClick={onReload}
                className="flex cursor-default items-center gap-2 px-1.5 py-1.5 text-left text-[11.5px] text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
              >
                <RefreshCw className="size-3.5" /> {t("frostmod.reloadGame")}
              </button>
              <button
                onClick={stop}
                className="flex cursor-default items-center gap-2 px-1.5 py-1.5 text-left text-[11.5px] text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
              >
                <Square className="size-3.5" /> {t("frostmod.stop")}
              </button>
            </>
          ) : (
            // Nothing to start until it is installed — the provider puts it in on first run.
            status?.installed && (
              <button
                onClick={start}
                className="flex cursor-default items-center gap-2 px-1.5 py-1.5 text-left text-[11.5px] text-primary transition-colors hover:bg-foreground/[0.05] hover:brightness-110"
              >
                <Play className="size-3.5" /> {t("frostmod.start")}
              </button>
            )
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
