import { useCallback, useEffect, useState, type Ref } from "react";
import { Settings as SettingsIcon, Play, Gamepad2, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import type { LoadedPlugin } from "@/lib/pluginHost";
import { useConfig } from "@frost/shared/Context/Config";
import { useGameRunning } from "../../lib/useGameRunning";
import { useT } from "@/i18n";
import { launchGame, contentLockAvailable, contentSecureAvailable } from "@frost/shared/api/mods";
import type { GameCaps } from "@frost/shared/types";
import type { StudioTab } from "../Studio/Studio";
import { RAIL, railItemFor, type DashboardView, type RailItem } from "./nav";
import DownloadQueue from "./DownloadQueue";
import TrackBuildBadge from "./TrackBuildBadge";
import Brand from "./Brand";
import ContextBar from "./ContextBar";
import WindowControls, { IS_MAC } from "./WindowControls";

/** MX Bikes takes a while to show up in the process list; stop saying "Starting…" after this. */
const STARTING_TIMEOUT_MS = 15000;

interface TopRailProps {
  view: DashboardView;
  studioTab: StudioTab;
  plugins: LoadedPlugin[];
  onNavigate: (view: DashboardView, studio?: StudioTab) => void;
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
export default function TopRail({ view, studioTab, plugins, onNavigate, leftRef, rightRef }: TopRailProps) {
  const t = useT();
  const { game } = useConfig();
  const caps = game.caps;
  const { running: gameRunning, refresh: refreshGame } = useGameRunning();
  const [starting, setStarting] = useState(false);

  const [hasLock, setHasLock] = useState(false);
  const [hasSecure, setHasSecure] = useState(false);
  useEffect(() => {
    contentLockAvailable().then(setHasLock).catch(() => {});
    contentSecureAvailable().then(setHasSecure).catch(() => {});
  }, []);

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

  const onPlay = useCallback(async () => {
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
  }, [t, refreshGame]);

  const items = visibleRail(plugins, (cap) => !cap || caps[cap], hasLock, hasSecure);
  const active = railItemFor(view, items);

  return (
    <>
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-[52px] flex-none select-none items-center border-b border-border bg-window",
        // Clear the space macOS reserves for its traffic-lights — and pad the trailing
        // edge there too, because mac draws none of our own window controls, so PLAY
        // would otherwise sit flush against the window edge.
        IS_MAC ? "pl-[82px] pr-4" : "pl-[18px]",
      )}
    >
      <Brand />

      <nav className="ml-7 flex h-full items-stretch gap-[22px]">
        {items.map((item) => {
          const on = active?.id === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.view, item.studio)}
              className={cn(
                "relative flex cursor-default items-center font-cond text-[14px] font-semibold uppercase tracking-[0.15em] transition-colors",
                on ? "text-foreground" : "text-muted-foreground hover:text-foreground",
              )}
            >
              {item.rawLabel ?? t(item.label)}
              {on && (
                <span className="u-skew absolute inset-x-[-3px] bottom-0 h-[3px] bg-primary" />
              )}
            </button>
          );
        })}
      </nav>

      <div data-tauri-drag-region className="flex-1" />

      <div className="flex items-center gap-1 text-muted-foreground">
        {/* A track compiles for minutes; this is what makes that visible from anywhere but
            the Studio, and the way back to it. */}
        <TrackBuildBadge onOpen={() => onNavigate("studio", "track")} />
        <DownloadQueue collapsed />
        <button
          onClick={() => onNavigate("settings")}
          title={t("nav.settings")}
          aria-label={t("nav.settings")}
          className={cn(
            "grid size-[30px] cursor-default place-items-center transition-colors hover:text-foreground",
            view === "settings" && "bg-popover text-foreground",
          )}
        >
          <SettingsIcon className="size-4" />
        </button>
      </div>

      <span className="mx-3.5 h-5 w-px bg-border" />

      <button
        data-tour="play"
        onClick={onPlay}
        disabled={gameRunning || starting}
        title={gameRunning ? t("game.running") : t("game.launch")}
        className={cn(
          "u-skew flex h-8 cursor-default items-center px-5 transition-colors",
          gameRunning || starting
            ? "border border-input text-muted-foreground"
            : "bg-primary text-primary-foreground hover:brightness-110 active:brightness-95",
        )}
      >
        <span className="u-unskew flex items-center gap-2">
          {gameRunning ? (
            <Gamepad2 className="size-3.5 text-success" />
          ) : starting ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Play className="size-3.5 fill-current" />
          )}
          <span className="font-cond text-[14.5px] font-bold uppercase tracking-[0.2em]">
            {gameRunning ? t("game.running") : starting ? t("game.starting") : t("game.play")}
          </span>
        </span>
      </button>

      <WindowControls className="ml-4" />
    </div>
    <ContextBar
      item={active}
      view={view}
      studioTab={studioTab}
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
  hasLock: boolean,
  hasSecure: boolean,
): RailItem[] {
  const ok = (g: { cap?: keyof GameCaps; needsLock?: boolean; needsSecure?: boolean }) =>
    hasCap(g.cap) && (!g.needsLock || hasLock) && (!g.needsSecure || hasSecure);

  const items: RailItem[] = RAIL.filter(ok).map((item) => {
    const tabs = item.tabs?.filter(ok);
    // A group whose tabs are all hidden still opens its own landing view, but a group whose
    // *first* tab is hidden must not land on it — Garage with no 3D viewer opens Presets.
    return { ...item, tabs, view: tabs?.length ? tabs[0].view : item.view, studio: tabs?.length ? tabs[0].studio : item.studio };
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
      studio: undefined,
      tabs: panels,
    });
  }
  return items;
}
