import { useCallback, useEffect, useRef, useState, type Ref } from "react";
import { Play, Gamepad2, Loader2, Flag, Zap, Rocket, Gauge } from "lucide-react";
import { useExtensionBrand } from "@frost/shared/extensions/ExtensionHost";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import type { LoadedPlugin } from "@frost/shared/lib/pluginHost";
import { useConfig } from "@frost/shared/Context/Config";
import { useGameRunning } from "../../lib/useGameRunning";
import { useT } from "@/i18n";
import { launchGame, launchStallReason } from "@frost/shared/api/mods";
import type { GameCaps } from "@frost/shared/types";
import { RAIL, railItemFor, type DashboardView, type RailItem } from "./nav";
import DownloadQueue from "./DownloadQueue";
import FrostmodBadge from "./FrostmodBadge";
import QueueBanner from "./QueueBanner";
import ContextBar from "./ContextBar";
import AppBar from "@frost/shared/Components/Shell/AppBar";

/** MX Bikes takes a while to show up in the process list; stop saying "Starting…" after this. */
const STARTING_TIMEOUT_MS = 15000;
/** A launch handed to Steam is given 45s by the backend to produce a game process; ask why
 *  it didn't just after that, so a launch that went nowhere ends in a reason, not silence. */
const STALL_CHECK_MS = 47000;

interface TopRailProps {
  view: DashboardView;
  plugins: LoadedPlugin[];
  /** Extensions' tabs, folded into one trailing rail item like plugins' panels. */
  extViews?: { route: string; title: string }[];
  onNavigate: (view: DashboardView) => void;
  /** Where the mounted screen portals its own toolbar — see `ContextBar`. */
  leftRef: Ref<HTMLDivElement>;
  rightRef: Ref<HTMLDivElement>;
}

/**
 * The app's chrome: brand, navigation, PLAY and the window controls, in one 52px row.
 *
 * This replaces the sidebar. A horizontal rail is what changes the app's silhouette away
 * from the generated-looking dashboard it was, and it hands the content area the full
 * window width — seven mod cards a row instead of five. The cost is that the rail fits
 * about seven items, which is why `nav.ts` groups Locker and Presets under Garage.
 *
 * The whole row is a drag region; every interactive child opts back out with
 * `data-tauri-drag-region={undefined}` by simply being a button — Tauri only drags from
 * elements carrying the attribute.
 */
export default function TopRail({ view, plugins, extViews = [], onNavigate, leftRef, rightRef }: TopRailProps) {
  const t = useT();
  const { game } = useConfig();
  const caps = game.caps;
  const { running: gameRunning, refresh: refreshGame } = useGameRunning();
  const [starting, setStarting] = useState(false);
  // An extension theme can rename the button and swap its icon; running/starting stay ours.
  const ext = useExtensionBrand();

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

  const stallTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (stallTimer.current) clearTimeout(stallTimer.current);
    },
    [],
  );

  const onPlay = useCallback(async () => {
    if (stallTimer.current) clearTimeout(stallTimer.current);
    setStarting(true);
    try {
      const outcome = await launchGame();
      if (outcome === "already_running") {
        toast.info(t("game.alreadyRunning"));
        setStarting(false);
      } else {
        toast.success(t("game.launching"));
        stallTimer.current = setTimeout(() => {
          launchStallReason()
            .then((reason) => {
              if (reason) toast.error(t("game.launchFailed"), { description: reason, duration: 20000 });
            })
            .catch(() => {});
        }, STALL_CHECK_MS);
      }
    } catch (e) {
      toast.error(t("game.launchFailed"), { description: String(e) });
      setStarting(false);
    }
    refreshGame();
  }, [t, refreshGame]);

  const items = visibleRail(plugins, (cap) => !cap || caps[cap], extViews);
  const active = railItemFor(view, items);

  return (
    <>
    <AppBar
      name={"MXB App"}
      tabs={items.map((item) => ({ id: item.id, label: item.rawLabel ?? t(item.label) }))}
      active={active?.id}
      onPick={(id) => {
        const item = items.find((i) => i.id === id);
        if (item) onNavigate(item.view);
      }}
      right={
        <>
          {/* What FrostMod is doing, and Start/Stop, from wherever you are. The sidebar
              carried this on every screen and the rail has to as well: its state changes
              while you are somewhere else, and Settings is four screens away. */}
          {/* The server line, visible on every tab: the turn can come while you're elsewhere. */}
          <QueueBanner />
          <FrostmodBadge />
          <DownloadQueue onOpenDownloads={() => onNavigate("downloads")} />
        </>
      }
      onSettings={() => onNavigate("settings")}
      settingsActive={view === "settings"}
      settingsLabel={t("nav.settings")}
      windowLabels={{
        minimize: t("window.minimize"),
        maximize: t("window.maximize"),
        close: t("window.close"),
      }}
      action={
        <button
          data-tour="play"
          onClick={onPlay}
          disabled={gameRunning || starting}
          title={gameRunning ? t("game.running") : t("game.launch")}
          className={cn(
            "mxb-play flex h-8 cursor-default items-center rounded-full px-6 transition-colors",
            gameRunning || starting
              ? "border border-input text-muted-foreground"
              : "bg-primary text-primary-foreground hover:brightness-110 active:brightness-95",
          )}
        >
          <span className="flex items-center gap-2">
            {gameRunning ? (
              <Gamepad2 className="size-3.5 text-success" />
            ) : starting ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <PlayIcon icon={ext.playIcon} />
            )}
            <span className="font-cond text-[14.5px] font-bold tracking-[-0.02em]">
              {gameRunning ? t("game.running") : starting ? t("game.starting") : (ext.playLabel ?? t("game.play"))}
            </span>
          </span>
        </button>
      }
    />
    <ContextBar
      item={active}
      view={view}
      onNavigate={onNavigate}
      leftRef={leftRef}
      rightRef={rightRef}
    />
    </>
  );
}

/**
 * The rail as this build and this game actually have it: capability-gated, with the
 * optional local modules honoured, and each plugin's panels folded into one trailing item.
 */
function visibleRail(
  plugins: LoadedPlugin[],
  hasCap: (cap?: keyof GameCaps) => boolean,
  extViews: { route: string; title: string }[] = [],
): RailItem[] {
  const ok = (g: { cap?: keyof GameCaps }) => hasCap(g.cap);

  const items: RailItem[] = RAIL.filter(ok).map((item) => {
    const tabs = item.tabs?.filter(ok);
    // A group whose tabs are all hidden still opens its own landing view, but a group whose
    // *first* tab is hidden must not land on it — Garage with no 3D viewer opens Presets.
    return { ...item, tabs, view: tabs?.length ? tabs[0].view : item.view };
  });

  if (plugins.length > 0) {
    const panels = plugins.flatMap((p) =>
      p.panels.map((panel) => ({
        view: `plugin:${p.manifest.id}/${panel.id}` as DashboardView,
        label: "plugins.section" as const,
        rawLabel: panel.label,
      })),
    );
    items.push({
      id: "plugins",
      label: "plugins.section",
      view: panels[0].view,
      tabs: panels,
    });
  }
  // Always there: the marketplace is how people find extensions in the first place. Each
  // installed extension's tabs follow it in the context bar.
  const extTabs = extViews.map((v, i) => ({
    view: v.route as DashboardView,
    label: "ext.section" as const,
    rawLabel: v.title,
    // One line, between the Marketplace and what was installed from it.
    dividerBefore: i === 0,
  }));
  items.push({
    id: "extensions",
    label: "ext.section",
    view: "extensions",
    tabs: [{ view: "extensions", label: "ext.marketplace" }, ...extTabs],
  });
  return items;
}

/** The Play button's icon: ours, or one from the fixed set an extension theme may pick. */
function PlayIcon({ icon }: { icon: string | null }) {
  switch (icon) {
    case "none":
      return null;
    case "flag":
      return <Flag className="size-3.5 fill-current" />;
    case "bolt":
      return <Zap className="size-3.5 fill-current" />;
    case "rocket":
      return <Rocket className="size-3.5" />;
    case "gauge":
      return <Gauge className="size-3.5" />;
    default:
      return <Play className="size-3.5 fill-current" />;
  }
}
