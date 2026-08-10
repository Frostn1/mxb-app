import { useCallback, useEffect, useState } from "react";
import {
  Home,
  Library as LibraryIcon,
  Bike,
  Shirt,
  Settings,
  RefreshCw,
  Play,
  Square,
  Loader2,
  Gamepad2,
  SlidersHorizontal,
  Store,
  Palette,
  PanelLeftClose,
  PanelLeftOpen,
  Server as ServerIcon,
  Plug,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useFrostmod } from "../../Context/FrostmodContext";
import { useInstall } from "../../Context/Install";
import { displayName } from "../../lib/mods";
import { useT, type TKey } from "../../i18n/context";
import {
  experimentalState,
  launchGame,
  onSyncEvent,
  type ExperimentalState,
  type SyncEvent,
} from "../../api/mods";
import { useGameRunning } from "../../lib/useGameRunning";
import { useConfig } from "../../Context/Config";
import type { GameCaps } from "../../types";
import JoinServerDialog from "./JoinServerDialog";

export type DashboardView =
  | "browse"
  | "shop"
  | "library"
  | "locker"
  | "presets"
  | "studio"
  | "servers"
  | "manage"
  | "settings";

interface SidebarProps {
  view: DashboardView;
  onNavigate: (view: DashboardView) => void;
}

/**
 * `cap` names a capability the active game must have for the entry to appear. Gating on a
 * capability rather than on the game id keeps "why is this hidden" answerable in one
 * place — and turning a feature on for another title is a single `true` in `game.rs`.
 */
type NavEntry = {
  id: DashboardView;
  label: TKey;
  icon: typeof Home;
  cap?: keyof GameCaps;
};

const NAV: NavEntry[] = [
  { id: "browse", label: "nav.browse", icon: Home },
  { id: "library", label: "nav.library", icon: LibraryIcon },
  // The Locker and Rider views are the 3D preview; GP Bikes' meshes need their own
  // part bindings before they can be shown.
  { id: "locker", label: "nav.locker", icon: Bike, cap: "viewer" },
  { id: "presets", label: "nav.presets", icon: Shirt },
  // Designer, Paints and Rider in one tab. Deliberately *not* viewer-gated: building a `.pnt`
  // is the same job for either title — same container, same encoder, same folders — and only
  // the 3D preview needs part bindings. Studio hides the sub-views that do.
  { id: "studio", label: "nav.studio", icon: Palette },
  { id: "manage", label: "nav.manage", icon: SlidersHorizontal, cap: "manage" },
];

/**
 * Sits second, next to Browse, because it is the other catalog — unlike the experimental
 * entry below, which is appended.
 *
 * Gated only on `cap: "shop"`: the store is mxbikes-shop.com, so it has nothing to sell a
 * player on another title. Deliberately *not* gated on the build-time catalog credential any
 * more — the view's other half is the account's own purchases, which needs nothing but the
 * user's login, so a build without the credential still has a working Shop tab. `Shop.tsx`
 * hides the Catalog half in that case and opens on purchases.
 */
const SHOP_ENTRY: NavEntry = { id: "shop", label: "nav.shop", icon: Store, cap: "shop" };

/** Shown only when the experimental features are on — see `settings.experimental`.
 *  Also capability-gated: it administers MX Bikes dedicated servers and keys riders by
 *  MX Bikes GUID, neither of which means anything for another title. */
const EXPERIMENTAL_NAV: NavEntry = {
  id: "servers",
  label: "nav.servers",
  icon: ServerIcon,
  cap: "servers",
};

/** Remembered across launches: a collapsed sidebar is a preference, not a mode. */
const COLLAPSED_KEY = "mxb:sidebarCollapsed:v1";

const IN_PROGRESS = new Set(["resolving", "downloading", "extracting", "placing"]);

/** MX Bikes takes a while to show up in the process list; stop saying "Starting…" after this. */
const STARTING_TIMEOUT_MS = 15000;

export default function Sidebar({ view, onNavigate }: SidebarProps) {
  const t = useT();
  const { running, reload, status, start, stop } = useFrostmod();
  const { active, queueLength } = useInstall();
  const { running: gameRunning, refresh: refreshGame } = useGameRunning();
  const { game } = useConfig();
  const caps = game.caps;
  const [starting, setStarting] = useState(false);
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem(COLLAPSED_KEY) === "1",
  );
  const toggleCollapsed = useCallback(
    () =>
      setCollapsed((c) => {
        localStorage.setItem(COLLAPSED_KEY, c ? "0" : "1");
        return !c;
      }),
    [],
  );
  const [joinOpen, setJoinOpen] = useState(false);
  // Re-read on navigation rather than subscribing: the toggle lives in Settings, and
  // leaving that page is exactly when the nav needs to reflect a change.
  const [experimental, setExperimental] = useState(false);

  // Kept whole rather than just `.enabled`: the sync line below reads the same payload, and
  // asking twice for one answer is two IPC round trips per navigation.
  const [sync, setSync] = useState<ExperimentalState | null>(null);
  const [syncBusy, setSyncBusy] = useState<SyncEvent["phase"] | null>(null);

  const readExperimental = useCallback(() => {
    experimentalState()
      .then((s) => {
        setExperimental(s.enabled);
        setSync(s);
      })
      .catch(() => {});
  }, []);
  useEffect(readExperimental, [readExperimental, view]);

  // Follow the background work as it happens, and re-read the record it leaves behind.
  useEffect(() => {
    const pending = onSyncEvent((e) => {
      setSyncBusy(e.phase === "publishing" || e.phase === "pulling" ? e.phase : null);
      if (e.phase !== "publishing" && e.phase !== "pulling") readExperimental();
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, [readExperimental]);

  // Two independent gates: servers needs the experimental toggle, and every entry needs the
  // active game to support it. Built here rather than inline so the JSX stays one `.map`.
  const nav = [
    NAV[0],
    SHOP_ENTRY,
    ...NAV.slice(1),
    ...(experimental ? [EXPERIMENTAL_NAV] : []),
  ].filter(({ cap }) => !cap || caps[cap]);

  // Drop out of "Starting…" once the game shows up — or once it's clear it isn't going
  // to, so a launch that failed silently doesn't leave the button stuck.
  useEffect(() => {
    if (!starting) return;
    if (gameRunning) {
      setStarting(false);
      return;
    }
    const id = setTimeout(() => setStarting(false), STARTING_TIMEOUT_MS);
    return () => clearTimeout(id);
  }, [starting, gameRunning]);

  const onPlay = async () => {
    setStarting(true);
    try {
      const outcome = await launchGame();
      if (outcome === "already_running") {
        toast.info(t("game.alreadyRunning"));
        setStarting(false);
      } else {
        toast.success(t("game.launching"));
      }
    } catch (e) {
      toast.error(t("game.launchFailed"), { description: String(e) });
      setStarting(false);
    }
    refreshGame();
  };

  const installing = active && IN_PROGRESS.has(active.stage);
  const pct =
    active?.total && active.received
      ? Math.round((active.received / active.total) * 100)
      : undefined;

  const onReload = async () => {
    const outcome = await reload();
    if (outcome === "signaled") toast.success(t("frostmod.reloadedGame"));
    else if (outcome === "not_running") toast.info(t("frostmod.notRunningToast"));
  };

  return (
    <aside
      className={cn(
        "flex flex-none flex-col border-r border-white/[0.06] bg-window pb-3 pt-3.5 transition-[width]",
        collapsed ? "w-[60px] px-2" : "w-[216px] px-2.5",
      )}
    >
      <div
        className={cn(
          "flex pb-3",
          collapsed ? "justify-center" : "items-start justify-between gap-2 px-3",
        )}
      >
        {!collapsed && (
          <div className="flex min-w-0 flex-col">
            <span className="text-[13px] font-bold tracking-[0.2px]">MXB App</span>
            {/* The switcher lives in Settings now, but which game you're driving still has
                to be visible — every list below it is scoped to that choice. */}
            <span className="truncate text-[11px] font-medium text-muted-foreground">
              {game.display}
            </span>
          </div>
        )}
        <button
          onClick={toggleCollapsed}
          title={t(collapsed ? "sidebar.expand" : "sidebar.collapse")}
          aria-label={t(collapsed ? "sidebar.expand" : "sidebar.collapse")}
          className="flex cursor-default items-center rounded-md p-1 text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
        >
          {collapsed ? (
            <PanelLeftOpen className="size-4" />
          ) : (
            <PanelLeftClose className="size-4" />
          )}
        </button>
      </div>

      <nav className="flex flex-col gap-0.5">
        {nav.map(({ id, label, icon: Icon }) => {
          const activeNav = view === id;
          return (
            <button
              key={id}
              data-tour={id}
              onClick={() => onNavigate(id)}
              title={collapsed ? t(label) : undefined}
              aria-label={collapsed ? t(label) : undefined}
              className={cn(
                "flex cursor-default items-center gap-2.5 rounded-lg py-2.5 text-[13.5px] transition-colors",
                collapsed ? "justify-center px-0" : "px-3",
                activeNav
                  ? "bg-accent font-semibold text-accent-foreground"
                  : "font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
              )}
            >
              <Icon className="size-4 flex-none" />
              {!collapsed && <span>{t(label)}</span>}
            </button>
          );
        })}
      </nav>

      <div className="mt-auto flex flex-col gap-2">
        {!collapsed && installing && (
          <div className="flex flex-col gap-[7px] rounded-[10px] border border-white/[0.07] bg-[color-mix(in_srgb,var(--card)_60%,var(--window))] px-3 py-2.5">
            <div className="flex items-baseline justify-between gap-2">
              <span className="truncate text-[11.5px] font-semibold text-foreground/85">
                {t("sidebar.installing", { name: displayName(active.title) })}
              </span>
              {pct !== undefined && (
                <span className="flex-none text-[10.5px] text-muted-foreground">
                  {pct}%
                </span>
              )}
            </div>
            {queueLength > 0 && (
              <span className="text-[10.5px] text-muted-foreground">
                {t("sidebar.queued", { count: queueLength })}
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

        <button
          data-tour="play"
          onClick={onPlay}
          disabled={gameRunning || starting}
          title={gameRunning ? t("game.running") : t("game.launch")}
          aria-label={collapsed ? t("game.play") : undefined}
          className={cn(
            "flex cursor-default items-center justify-center gap-2 rounded-lg px-3 py-2.5 text-[13.5px] font-semibold transition-colors",
            gameRunning || starting
              ? "border border-white/[0.07] text-muted-foreground"
              : "bg-primary text-primary-foreground hover:brightness-105 active:brightness-95",
          )}
        >
          {gameRunning ? (
            <Gamepad2 className="size-4 text-success" />
          ) : starting ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Play className="size-4" />
          )}
          {!collapsed && (
            <span>
              {gameRunning
                ? t("game.running")
                : starting
                  ? t("game.starting")
                  : t("game.play")}
            </span>
          )}
        </button>

        {/* Join-by-address launches the game with `-directconnect`. Behind the experimental
            toggle with the rest of the unfinished multiplayer surface: the flag is
            undocumented by PiBoSo, so it's opt-in rather than something a player finds by
            accident. Capability-gated on top — both the argv parser offset it was found at
            and the default port it assumes are MX Bikes', so it waits until GP's are
            confirmed. */}
        {!collapsed && experimental && caps.joinByAddress && (
        <>
        <button
          onClick={() => setJoinOpen(true)}
          disabled={gameRunning}
          title={gameRunning ? t("game.running") : t("join.title")}
          className={cn(
            "flex cursor-default items-center justify-center gap-2 rounded-lg border border-white/[0.07] px-3 py-1.5 text-[12px] font-medium transition-colors",
            gameRunning
              ? "text-muted-foreground"
              : "text-foreground/80 hover:bg-white/[0.04]",
          )}
        >
          <Plug className="size-3.5" />
          <span>{t("join.title")}</span>
        </button>

        <JoinServerDialog
          open={joinOpen}
          onOpenChange={setJoinOpen}
          onJoined={refreshGame}
        />
        </>
        )}

        {/* Paint sync, in one line. Both halves of it run in the background off actions the
            player didn't ask for — an apply, a launch, the game rewriting profile.ini — so
            without something like this the only place its state existed was the log file.
            Shown only once there's an account, since before that the Servers page is where
            the answer is. */}
        {!collapsed && experimental && sync?.enrolled && (
          <div className="flex items-center gap-2 rounded-[10px] border border-white/[0.07] px-3 py-2">
            <span
              className={cn(
                "size-[7px] flex-none rounded-full",
                syncBusy
                  ? "animate-pulse bg-primary"
                  : sync.sync.publishedAt
                    ? "bg-success"
                    : "bg-warning",
              )}
            />
            <span className="flex-1 truncate text-[11.5px] text-muted-foreground">
              {syncBusy === "publishing"
                ? t("sync.publishing")
                : syncBusy === "pulling"
                  ? t("sync.pulling")
                  : sync.sync.publishedAt
                    ? t("sync.sidebarOk", { count: sync.sync.pulledRiders })
                    : t("sync.sidebarUnpublished")}
            </span>
            <button
              onClick={() => onNavigate("servers")}
              title={t("sync.title")}
              className="cursor-default text-muted-foreground transition-colors hover:text-foreground"
            >
              <Shirt className="size-3.5" />
            </button>
          </div>
        )}

        {/* FrostMod is a compiled MX Bikes plugin — there is nothing to report, start or
            reload for a title it wasn't built for. */}
        {caps.frostmod && (
        <div
          data-tour="frostmod"
          title={
            collapsed
              ? running === null
                ? t("frostmod.checking")
                : running
                  ? t("frostmod.running")
                  : t("frostmod.notRunning")
              : undefined
          }
          className={cn(
            "flex items-center rounded-[10px] border border-white/[0.07] py-2",
            collapsed ? "justify-center gap-1.5 px-1" : "gap-2 px-3",
          )}
        >
          <span
            className={cn(
              "size-[7px] flex-none rounded-full",
              running ? "bg-success" : "bg-muted-foreground/50",
            )}
          />
          {!collapsed && (
            <span className="flex-1 text-[11.5px] text-muted-foreground">
              {running === null
                ? t("frostmod.checking")
                : running
                  ? t("frostmod.running")
                  : t("frostmod.notRunning")}
            </span>
          )}
          {running ? (
            <>
              <button
                onClick={onReload}
                title={t("frostmod.reloadGame")}
                className="cursor-default text-muted-foreground transition-colors hover:text-foreground"
              >
                <RefreshCw className="size-3.5" />
              </button>
              {!collapsed && (
                <button
                  onClick={stop}
                  title={t("frostmod.stop")}
                  className="cursor-default text-muted-foreground transition-colors hover:text-foreground"
                >
                  <Square className="size-3.5" />
                </button>
              )}
            </>
          ) : (
            status?.installed && (
              <button
                onClick={start}
                title={t("frostmod.start")}
                className="cursor-default text-primary transition-colors hover:brightness-110"
              >
                <Play className="size-3.5" />
              </button>
            )
          )}
        </div>
        )}

        <button
          data-tour="settings"
          onClick={() => onNavigate("settings")}
          title={collapsed ? t("nav.settings") : undefined}
          aria-label={collapsed ? t("nav.settings") : undefined}
          className={cn(
            "flex cursor-default items-center gap-2.5 rounded-lg py-2.5 text-[13.5px] transition-colors",
            collapsed ? "justify-center px-0" : "px-3",
            view === "settings"
              ? "bg-accent font-semibold text-accent-foreground"
              : "font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
          )}
        >
          <Settings className="size-4 flex-none" />
          {!collapsed && <span>{t("nav.settings")}</span>}
        </button>
      </div>
    </aside>
  );
}
