import { useCallback, useEffect, useState } from "react";
import type { LoadedPlugin } from "@/lib/pluginHost";
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
  ShoppingBag,
  Brush,
  Move3d,
  Mountain,
  Palette,
  PersonStanding,
  Puzzle,
  Shield,
  PanelLeftClose,
  PanelLeftOpen,
  Plug,
  Download as DownloadIcon,
  ChevronDown,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useFrostmod } from "../../Context/FrostmodContext";
import { useDownloads } from "../../Context/Downloads";
import { useT, type TFunc, type TKey } from "../../i18n/context";
import {
  experimentalState,
  cpServers,
  launchGame,
  onSyncEvent,
  type ExperimentalState,
  type SyncEvent,
} from "../../api/mods";
import { useGameRunning } from "../../lib/useGameRunning";
import { useConfig } from "../../Context/Config";
import type { GameCaps } from "../../types";
import { ATTACH_PROBLEM } from "../../types";
import { contentLockAvailable } from "../../api/mods";
import type { StudioTab } from "../Studio/Studio";
import JoinServerDialog from "./JoinServerDialog";
import DownloadQueue from "./DownloadQueue";

/**
 * A page in the shell. The template literal is how a plugin gets a nav row: its panels are
 * addressed `plugin:<plugin id>/<panel id>`, so the shell can route to one without the
 * union having to name plugins it will never know about at build time.
 */
export type DashboardView =
  | `plugin:${string}`
  | "browse"
  | "shop"
  | "hub"
  | "library"
  | "downloads"
  | "locker"
  | "presets"
  | "studio"
  | "manage"
  | "settings";

interface SidebarProps {
  view: DashboardView;
  /** Which Studio sub-view is showing, so the right child row reads as active. */
  studioTab: StudioTab;
  /** Plugins running this session. Each contributes its panels as rows under one group. */
  plugins: LoadedPlugin[];
  onNavigate: (view: DashboardView, studio?: StudioTab) => void;
}

/**
 * `cap` names a capability the active game must have for the entry to appear. Gating on a
 * capability rather than on the game id keeps "why is this hidden" answerable in one
 * place — and turning a feature on for another title is a single `true` in `game.rs`.
 */
type NavEntry = {
  id: DashboardView;
  /** A translation key for the app's own rows. A plugin supplies `rawLabel` instead. */
  label: TKey;
  /** A label the app cannot translate, because a plugin wrote it. Wins over `label`. */
  rawLabel?: string;
  icon: typeof Home;
  cap?: keyof GameCaps;
  /** Indented under this entry, behind a chevron. */
  children?: NavEntry[];
  /**
   * Which Studio sub-view this row opens. Studio's tools are entries in the sidebar rather
   * than a control inside the page: there are six of them and a row across the top has
   * nowhere to grow, while a list under Studio reads the way Downloads reads under Library.
   *
   * They all share `id: "studio"`, so the active row is the one whose sub-view is showing
   * rather than the one whose id matches.
   */
  studio?: StudioTab;
  /** Hidden unless the optional local content-lock module is present. */
  needsLock?: boolean;
};

/** A row's text. A plugin wrote its own, so there is nothing to translate. */
const entryLabel = (t: TFunc, e: NavEntry) => e.rawLabel ?? t(e.label);

const NAV: NavEntry[] = [
  { id: "browse", label: "nav.browse", icon: Home },
  {
    id: "library",
    label: "nav.library",
    icon: LibraryIcon,
    // Under the library rather than beside it: Downloads answers the question the library
    // can't — which of these arrived today, and what didn't arrive at all — so it reads as
    // part of the same place, not a seventh destination competing with it.
    children: [{ id: "downloads", label: "nav.downloads", icon: DownloadIcon }],
  },
  // The Locker and Rider views are the 3D preview; GP Bikes' meshes need their own
  // part bindings before they can be shown.
  { id: "locker", label: "nav.locker", icon: Bike, cap: "viewer" },
  { id: "presets", label: "nav.presets", icon: Shirt },
  // Designer, Paints and Rider in one tab. Deliberately *not* viewer-gated: building a `.pnt`
  // is the same job for either title — same container, same encoder, same folders — and only
  // the 3D preview needs part bindings. Studio hides the sub-views that do.
  {
    id: "studio",
    label: "nav.studio",
    icon: Palette,
    children: [
      { id: "studio", label: "nav.designer", icon: Palette, studio: "designer" },
      { id: "studio", label: "nav.paints", icon: Brush, studio: "paints" },
      { id: "studio", label: "nav.rider", icon: PersonStanding, studio: "rider", cap: "viewer" },
      { id: "studio", label: "nav.pose", icon: Move3d, studio: "pose", cap: "viewer" },
      { id: "studio", label: "nav.track", icon: Mountain, studio: "track" },
      { id: "studio", label: "nav.protect", icon: Shield, studio: "protect", needsLock: true },
    ],
  },
  { id: "manage", label: "nav.manage", icon: SlidersHorizontal, cap: "manage" },
];

/**
 * Sits second, next to Browse, because it is the other catalog.
 *
 * Gated only on `cap: "shop"`: the store is mxbikes-shop.com, so it has nothing to sell a
 * player on another title. Deliberately *not* gated on the build-time catalog credential any
 * more — the view's other half is the account's own purchases, which needs nothing but the
 * user's login, so a build without the credential still has a working Shop tab. `Shop.tsx`
 * hides the Catalog half in that case and opens on purchases.
 */
const SHOP_ENTRY: NavEntry = { id: "shop", label: "nav.shop", icon: Store, cap: "shop" };

/**
 * MXB Hub — the other store, and the same reasoning as the entry above: it sells MX Bikes
 * mods, so it has nothing to offer a player on another title.
 *
 * Its own entry rather than a third tab inside Shop. The two are separate storefronts with
 * separate accounts and separate carts, and folding them into one view would mean a sign-in
 * that silently means a different thing depending on which pill is lit.
 */
const HUB_ENTRY: NavEntry = { id: "hub", label: "nav.hub", icon: ShoppingBag, cap: "shop" };

/** Remembered across launches: a collapsed sidebar is a preference, not a mode. */
const COLLAPSED_KEY = "mxb:sidebarCollapsed:v1";

/** Same idea, for the nav groups the user left open. */
const OPEN_GROUPS_KEY = "mxb:sidebarOpenGroups:v1";

/** MX Bikes takes a while to show up in the process list; stop saying "Starting…" after this. */
const STARTING_TIMEOUT_MS = 15000;

/**
 * One row of the nav.
 *
 * A row with children carries a chevron beside its label, and its children are indented under
 * it — except when the sidebar is collapsed, where there is nothing to indent into and a group
 * is simply its icons one after another.
 */
function NavRow({
  entry,
  active,
  collapsed,
  child,
  badge,
  group,
  onSelect,
}: {
  entry: NavEntry;
  active: boolean;
  collapsed: boolean;
  child: boolean;
  /** Unseen download failures to flag on this row; 0 for none. */
  badge: number;
  /** Only on a row that has children. */
  group?: { open: boolean; onToggle: () => void };
  onSelect: () => void;
}) {
  const t = useT();
  const Icon = entry.icon;
  const indented = child && !collapsed;
  return (
    // The pill is the row, not the button inside it, so the active background and the hover
    // reach under the chevron too.
    <div
      data-tour={entry.id}
      className={cn(
        "relative flex items-center rounded-lg transition-colors",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
      )}
    >
      <button
        onClick={onSelect}
        title={collapsed ? entryLabel(t, entry) : undefined}
        aria-label={collapsed ? entryLabel(t, entry) : undefined}
        className={cn(
          "flex min-w-0 flex-1 cursor-default items-center gap-2.5",
          collapsed ? "justify-center px-0 py-2.5" : indented ? "py-2 pl-9 pr-3" : "px-3 py-2.5",
          indented ? "text-[13px]" : "text-[13.5px]",
          active ? "font-semibold" : "font-medium",
        )}
      >
        <Icon className={cn("flex-none", indented ? "size-3.5" : "size-4")} />
        {!collapsed && <span className="truncate">{entryLabel(t, entry)}</span>}
      </button>

      {/* A failed download used to exist only as a toast, so one dismissed in passing left no
          sign anything had gone wrong. This is that sign — and it sits on the parent while the
          group is shut, because closing a group must not hide the failure with it. */}
      {badge > 0 && (
        <span
          title={t("downloads.failedBadge", { count: badge })}
          className={cn(
            "flex-none rounded-full bg-destructive text-center text-[10px] font-bold leading-[16px] text-destructive-foreground",
            collapsed ? "absolute right-1.5 top-1.5 size-2 p-0" : "mr-2.5 min-w-[18px] px-1",
          )}
        >
          {!collapsed && badge}
        </span>
      )}

      {group && !collapsed && (
        <button
          onClick={group.onToggle}
          title={t(group.open ? "sidebar.hideGroup" : "sidebar.showGroup", {
            name: entryLabel(t, entry),
          })}
          aria-label={t(group.open ? "sidebar.hideGroup" : "sidebar.showGroup", {
            name: entryLabel(t, entry),
          })}
          aria-expanded={group.open}
          className="flex flex-none cursor-default items-center py-2.5 pl-1 pr-2.5"
        >
          <ChevronDown
            className={cn("size-3.5 transition-transform", !group.open && "-rotate-90")}
          />
        </button>
      )}
    </div>
  );
}

export default function Sidebar({ view, studioTab, plugins, onNavigate }: SidebarProps) {
  const t = useT();
  // Locking needs an optional local module. Without it the Protect row would be a place you
  // can go and nothing can happen, so it isn't listed.
  const [hasLock, setHasLock] = useState(false);
  useEffect(() => {
    contentLockAvailable()
      .then(setHasLock)
      .catch(() => {});
  }, []);
  const { running, attachment, reload, status, start, stop } = useFrostmod();
  // FrostMod is up but isn't reaching the game — see `frostmod::attachment`. The good
  // states (and the grace period after a launch) deliberately look like plain "Running".
  const attachProblem =
    attachment !== null && ATTACH_PROBLEM.includes(attachment.state);
  const { unseenFailures } = useDownloads();
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
  const [openGroups, setOpenGroups] = useState<string[]>(() =>
    (localStorage.getItem(OPEN_GROUPS_KEY) ?? "").split(",").filter(Boolean),
  );
  const setGroupOpen = useCallback((id: DashboardView, open: boolean) => {
    setOpenGroups((cur) => {
      if (cur.includes(id) === open) return cur;
      const next = open ? [...cur, id] : cur.filter((g) => g !== id);
      localStorage.setItem(OPEN_GROUPS_KEY, next.join(","));
      return next;
    });
  }, []);
  const [joinOpen, setJoinOpen] = useState(false);
  // Re-read on navigation rather than subscribing: paint sync can be switched off in
  // Settings, and leaving that page is exactly when this line needs to reflect it.
  const [sync, setSync] = useState<ExperimentalState | null>(null);
  const [syncBusy, setSyncBusy] = useState<SyncEvent["phase"] | null>(null);

  const readExperimental = useCallback(() => {
    experimentalState().then(setSync).catch(() => {});
  }, []);
  useEffect(readExperimental, [readExperimental, view]);

  // Whether there is anywhere to join. The dialog can only offer servers the control plane
  // knows about, and the real list — the one the game's own browser shows — comes from
  // PiBoSo's master server, which we cannot read yet (see `tasks/mxb-server-browser.md`).
  // A button that opens onto an empty list and an IP box is worse than no button, so it
  // waits until there is something behind it. When the public browser lands, this gate is
  // what brings the button back on its own.
  const [joinable, setJoinable] = useState(false);
  useEffect(() => {
    cpServers()
      .then((list) => setJoinable(list.length > 0))
      .catch(() => setJoinable(false));
  }, [view]);

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

  // Every entry needs the
  // active game to support it. Built here rather than inline so the JSX stays one `.map`.
  const supported = ({ cap, needsLock }: NavEntry) =>
    (!cap || caps[cap]) && (!needsLock || hasLock);

  // Plugin rows, under one group so a paid add-on reads as a thing the user installed
  // rather than as another built-in page. Absent entirely when nothing is licensed.
  const pluginGroup: NavEntry[] =
    plugins.length === 0
      ? []
      : [
          {
            id: `plugin:${plugins[0].manifest.id}/${plugins[0].panels[0].id}` as DashboardView,
            label: "plugins.section",
            icon: Puzzle,
            children: plugins.flatMap((p) =>
              p.panels.map((panel) => ({
                id: `plugin:${p.manifest.id}/${panel.id}` as DashboardView,
                label: "plugins.section" as TKey,
                rawLabel: panel.label,
                icon: Puzzle,
              })),
            ),
          },
        ];
  const nav = [NAV[0], SHOP_ENTRY, HUB_ENTRY, ...NAV.slice(1), ...pluginGroup]
    .filter(supported)
    .map((e) => (e.children ? { ...e, children: e.children.filter(supported) } : e));

  // Flattened to the rows actually on screen, so the JSX below stays one `.map`: a group
  // contributes its own row, then its children when it's open — or always when the sidebar is
  // collapsed, where there is no indent to hide them behind.
  const rows = nav.flatMap((entry) => {
    const kids = entry.children ?? [];
    // Open because the user opened it, or because one of its pages is the one on screen —
    // arriving at Downloads from a toast shouldn't leave it hidden inside a shut group.
    const open = kids.some((k) => k.id === view) || openGroups.includes(entry.id);
    const shown = collapsed || open ? kids : [];
    const failures = (e: NavEntry) => (e.id === "downloads" ? unseenFailures : 0);
    return [
      {
        entry,
        child: false,
        // Whatever the children would have flagged, while they aren't on screen to flag it.
        badge: shown.length ? failures(entry) : Math.max(...kids.map(failures), failures(entry)),
        group: kids.length ? { open, onToggle: () => setGroupOpen(entry.id, !open) } : undefined,
      },
      ...shown.map((kid) => ({
        entry: kid,
        child: true,
        badge: failures(kid),
        group: undefined,
      })),
    ];
  });

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

      {/* The list scrolls; the Play button below it does not. Studio's tools are rows now,
          so a short window or an expanded group can push the list past the bottom, and the
          thing that must never scroll away is the one people came to press. */}
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto overflow-x-hidden">
        {rows.map(({ entry, child, badge, group }) => (
          <NavRow
            key={entry.studio ?? entry.id}
            entry={entry}
            active={
              entry.studio ? view === "studio" && studioTab === entry.studio : view === entry.id
            }
            collapsed={collapsed}
            child={child}
            badge={badge}
            group={group}
            onSelect={() => {
              onNavigate(entry.id, entry.studio);
              // Opening the parent's page shows what's under it. Only ever opens: clicking
              // Library twice shouldn't make Downloads disappear — that's the chevron's job.
              if (group) setGroupOpen(entry.id, true);
            }}
          />
        ))}
      </nav>

      <div className="mt-3 flex flex-none flex-col gap-2">
        {/* The install card is the queue's trigger now — same look, opens the panel. */}
        <DownloadQueue collapsed={collapsed} />

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

        {/* Join-by-address launches the game with `-directconnect`. Joining a server is not
            hosting one, so it is not hidden with the server-creation surface — but it only
            appears when there is a real list to show. Capability-gated too: both the argv
            parser offset it was found at and the default port it assumes are MX Bikes', so
            it waits until GP's are confirmed. */}
        {!collapsed && joinable && caps.joinByAddress && (
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
            No longer behind the experimental toggle: sync is on for everyone and works on
            any server, so everyone needs somewhere to see it working. Waits for an account,
            which the app claims for itself the first time sync runs. */}
        {!collapsed && sync?.paintSyncEnabled && sync?.enrolled && (
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
            {/* Where the detail is: Settings → Paint sync, which holds the publish and
                pull state, the GUID, and any paint the sync declined to overwrite. */}
            <button
              onClick={() => onNavigate("settings")}
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
          // A running FrostMod that never got into the game is the state this pill used
          // to report as plain "Running", which is exactly as far as the player could get
          // in working out why nothing was happening in game. The reason goes in the
          // tooltip whether or not the sidebar is collapsed.
          title={
            attachProblem
              ? attachment?.reason
              : collapsed
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
              attachProblem
                ? "bg-warning"
                : running
                  ? "bg-success"
                  : "bg-muted-foreground/50",
            )}
          />
          {!collapsed && (
            <span
              className={cn(
                "flex-1 text-[11.5px]",
                attachProblem ? "text-warning" : "text-muted-foreground",
              )}
            >
              {attachProblem
                ? t("frostmod.notInGame")
                : running === null
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
