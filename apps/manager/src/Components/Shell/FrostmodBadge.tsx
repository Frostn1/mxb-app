import { RefreshCw, Zap } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { useConfig } from "@frost/shared/Context/Config";
import { useFrostmod } from "../../Context/FrostmodContext";
import { useT } from "@/i18n";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";

/**
 * The game-loaded FrostMod plugin's connection state in the rail.
 *
 * The old sidebar carried this as a pill on every screen, and the rail that replaced it
 * didn't take it along, and the tour step that points at `[data-tour="frostmod"]` had
 * nothing to point at.
 *
 * It has to be visible from everywhere for the same reason the track build badge does: what
 * it reports changes while you are somewhere else. A dot alone would be a status light, so
 * the panel behind it explains the state and offers Reload only while the plugin is active.
 */
export default function FrostmodBadge() {
  const t = useT();
  const { game } = useConfig();
  const { integrationChoice, attachment, reload, status } = useFrostmod();

  // FrostMod is a compiled MX Bikes plugin — there is nothing to report, start or reload
  // for a title it wasn't built for.
  if (!game.caps.frostmod || integrationChoice !== "enabled") return null;

  // The plugin's handshake is the authority. `running` is intentionally absent here: it
  // describes the retired launcher/injector process and is normally false in plugin-only mode.
  const pluginActive = attachment?.state === "attached";
  const pluginProblem = attachment?.state === "not_attached" || attachment?.state === "blocked";
  const checking = status === null || attachment === null;
  const label = checking
    ? t("frostmod.checking")
    : !status.installed
      ? t("frostmod.notInstalled")
      : pluginActive
        ? t("frostmod.running")
        : attachment.state === "game_not_running"
          ? t("frostmod.notInGame")
          : attachment.state === "attaching"
            ? t("frostmod.connecting")
            : pluginProblem
              ? t("frostmod.pluginNotConnected")
              : t("frostmod.checkUnavailable");

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
          title={pluginProblem && attachment?.reason ? attachment.reason : label}
          aria-label={label}
          className="relative flex cursor-default items-center justify-center rounded-lg px-1.5 py-2.5 text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
        >
          <Zap className="size-4" />
          <span
            className={cn(
              "absolute bottom-1.5 right-1 size-[6px] rounded-full ring-2 ring-window",
              checking
                ? "bg-muted-foreground"
                : pluginProblem
                  ? "bg-warning"
                  : pluginActive
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
              checking
                ? "bg-muted-foreground"
                : pluginProblem
                  ? "bg-warning"
                  : pluginActive
                    ? "bg-success"
                    : "bg-muted-foreground/50",
            )}
          />
          <span className="truncate text-[12px] font-bold">{label}</span>
        </div>

        {pluginProblem && attachment?.reason && (
          <p className="border-b border-white/[0.07] px-3.5 py-2 text-[11px] text-muted-foreground">
            {attachment.reason}
          </p>
        )}

        <div className="flex flex-col gap-1 p-2">
          {pluginActive && (
            <button
              onClick={onReload}
              className="flex cursor-default items-center gap-2 px-1.5 py-1.5 text-left text-[11.5px] text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
            >
              <RefreshCw className="size-3.5" /> {t("frostmod.reloadGame")}
            </button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
