import {
  Home,
  Store,
  Library as LibraryIcon,
  Bike,
  Shirt,
  User,
  Settings,
  RefreshCw,
  Play,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useConfig } from "../../Context/Config";
import { useFrostmod } from "../../Context/FrostmodContext";
import { useInstall } from "../../Context/Install";
import { displayName } from "../../lib/mods";

export type DashboardView =
  | "browse"
  | "shop"
  | "library"
  | "locker"
  | "presets"
  | "rider"
  | "settings";

interface SidebarProps {
  view: DashboardView;
  onNavigate: (view: DashboardView) => void;
}

const NAV: { id: DashboardView; label: string; icon: typeof Home }[] = [
  { id: "browse", label: "Browse", icon: Home },
  { id: "shop", label: "Shop", icon: Store },
  { id: "library", label: "Library", icon: LibraryIcon },
  { id: "locker", label: "Locker", icon: Bike },
  { id: "presets", label: "Presets", icon: Shirt },
  { id: "rider", label: "Rider", icon: User },
  { id: "settings", label: "Settings", icon: Settings },
];

const IN_PROGRESS = new Set(["resolving", "downloading", "extracting", "placing"]);

/** Short "…\PiBoSo\MX Bikes" form of a long game path. */
function shortPath(p: string): string {
  if (!p) return "Not set";
  const parts = p.split(/[/\\]/).filter(Boolean);
  const tail = parts.slice(-2).join("\\");
  return parts.length > 2 ? `…\\${tail}` : tail;
}

export default function Sidebar({ view, onNavigate }: SidebarProps) {
  const { config } = useConfig();
  const { running, reload, status, start } = useFrostmod();
  const { active, queueLength } = useInstall();

  const installing = active && IN_PROGRESS.has(active.stage);
  const pct =
    active?.total && active.received
      ? Math.round((active.received / active.total) * 100)
      : undefined;

  const onReload = async () => {
    const outcome = await reload();
    if (outcome === "signaled") toast.success("FrostMod reloaded the game.");
    else if (outcome === "not_running") toast.info("FrostMod isn't running.");
  };

  return (
    <aside className="flex w-[216px] flex-none flex-col border-r border-white/[0.06] bg-window px-2.5 pb-3 pt-3.5">
      <nav className="flex flex-col gap-0.5">
        {NAV.map(({ id, label, icon: Icon }) => {
          const activeNav = view === id;
          return (
            <button
              key={id}
              onClick={() => onNavigate(id)}
              className={cn(
                "flex cursor-default items-center gap-2.5 rounded-lg px-3 py-2.5 text-[13.5px] transition-colors",
                activeNav
                  ? "bg-accent font-semibold text-accent-foreground"
                  : "font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
              )}
            >
              <Icon className="size-4" />
              <span>{label}</span>
            </button>
          );
        })}
      </nav>

      <div className="mt-auto flex flex-col gap-2">
        {installing && (
          <div className="flex flex-col gap-[7px] rounded-[10px] border border-white/[0.07] bg-[color-mix(in_srgb,var(--card)_60%,var(--window))] px-3 py-2.5">
            <div className="flex items-baseline justify-between gap-2">
              <span className="truncate text-[11.5px] font-semibold text-foreground/85">
                Installing “{displayName(active.title)}”
              </span>
              {pct !== undefined && (
                <span className="flex-none text-[10.5px] text-muted-foreground">
                  {pct}%
                </span>
              )}
            </div>
            {queueLength > 0 && (
              <span className="text-[10.5px] text-muted-foreground">
                +{queueLength} queued
              </span>
            )}
            <div className="h-[3px] overflow-hidden rounded-full bg-foreground/[0.08]">
              <div
                className={cn(
                  "h-full rounded-full bg-primary transition-[width]",
                  pct === undefined &&
                    "w-1/3 animate-[frost-indeterminate_1.2s_ease-in-out_infinite]",
                )}
                style={pct !== undefined ? { width: `${pct}%` } : undefined}
              />
            </div>
          </div>
        )}

        <div className="flex items-center gap-2 rounded-[10px] border border-white/[0.07] px-3 py-2">
          <span
            className={cn(
              "size-[7px] flex-none rounded-full",
              running ? "bg-success" : "bg-muted-foreground/50",
            )}
          />
          <span className="flex-1 text-[11.5px] text-muted-foreground">
            {running === null
              ? "Checking FrostMod…"
              : running
                ? "FrostMod running"
                : "FrostMod not running"}
          </span>
          {running ? (
            <button
              onClick={onReload}
              title="Reload game"
              className="cursor-default text-muted-foreground transition-colors hover:text-foreground"
            >
              <RefreshCw className="size-3.5" />
            </button>
          ) : (
            status?.installed && (
              <button
                onClick={start}
                title="Start FrostMod"
                className="cursor-default text-primary transition-colors hover:brightness-110"
              >
                <Play className="size-3.5" />
              </button>
            )
          )}
        </div>

        <div
          className="truncate px-1 font-mono text-[10px] text-faint"
          title={config.modsPath}
        >
          {shortPath(config.modsPath)}
        </div>
      </div>
    </aside>
  );
}
