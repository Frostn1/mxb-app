import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import HelpHint from "../ui/help-hint";
import { Segmented } from "../ui/segmented";
import { useT } from "../../i18n/context";
import { useConfig } from "../../Context/Config";
import { contentLockAvailable } from "../../api/mods";
import type { Loadout } from "../../types";
import Designer from "./Designer/Designer";
import PaintStudio from "../PaintStudio/PaintStudio";
import RiderStudio from "../Rider/RiderStudio";
import PoseStudio from "../Rider/PoseStudio";
import Protect from "./Protect/Protect";
import TrackStudio from "./TrackStudio/TrackStudio";
import RiderKitProvider from "../Rider/RiderKit";

/**
 * The Studio: everything that makes something, under one tab.
 *
 * Designer, Paints and Rider were three sidebar entries answering three halves of the same
 * question — draw a paint, pack a paint, put one on a rider — and the sidebar was growing an
 * entry per tool. They share a destination picker, a 3D preview and a mental model, so they
 * share a tab, and the sidebar goes back to being a list of places rather than a list of
 * features.
 *
 * The active sub-view is owned by the Dashboard, not here: Presets' "View in Rider" and the
 * first-run tour both need to open the Studio *at* a particular one.
 */

export type StudioTab = "designer" | "paints" | "rider" | "pose" | "track" | "protect";

interface StudioProps {
  tab: StudioTab;
  onTab: (tab: StudioTab) => void;
  /** A preset handed over by the Presets tab, for the Rider sub-view to load. */
  riderPreset: Loadout | null;
  /** The bike that preset was built against — a loadout doesn't name one. */
  riderBike: string | null;
  onRiderPresetLoaded: () => void;
}

export default function Studio({
  tab,
  onTab,
  riderPreset,
  riderBike,
  onRiderPresetLoaded,
}: StudioProps) {
  const t = useT();
  const { game } = useConfig();
  // The Rider sub-view is the 3D rider rig, and GP Bikes' rig has no part bindings — so it
  // isn't offered there rather than being offered and empty. Designer and Paints both work
  // for either title: a `.pnt` is a `.pnt`.
  const hasRider = game.caps.viewer;
  // Locking needs the optional local module, the same one the bike preview needs. Without
  // it the tab would be a button that can only ever fail, so it isn't offered.
  const [hasLock, setHasLock] = useState(false);
  useEffect(() => {
    contentLockAvailable()
      .then(setHasLock)
      .catch(() => {});
  }, []);
  // Which sub-views have ever been opened. A tab is mounted on first visit and then kept —
  // see the note on `Pane` below — so this is what stops someone who never opens the Rider
  // paying for its geometry.
  const [visited, setVisited] = useState<Set<StudioTab>>(() => new Set([tab]));
  // Sheet paths on their way from Paint Studio to the Designer. Held here because the two
  // sub-views never see each other, and cleared by the Designer once it has read them.
  const [handoff, setHandoff] = useState<string[] | null>(null);
  useEffect(() => setVisited((v) => (v.has(tab) ? v : new Set(v).add(tab))), [tab]);

  // Switching titles can take the Rider away underneath us, and the lock tab only
  // exists once the availability check has come back.
  useEffect(() => {
    if (!hasRider && (tab === "rider" || tab === "pose")) onTab("designer");
    if (!hasLock && tab === "protect") onTab("designer");
  }, [hasRider, hasLock, tab, onTab]);

  // One help hint, on whichever sub-view is open. Each had its own before they shared a tab,
  // and three "?" icons for one screen would be three ways to ask the same question.
  const help =
    tab === "designer"
      ? { title: t("nav.designer"), body: t("designer.help") }
      : tab === "paints"
        ? { title: t("nav.paints"), body: t("paints.help") }
        : tab === "pose"
          ? { title: t("nav.pose"), body: t("pose.help") }
          : tab === "track"
            ? { title: t("nav.track"), body: t("track.help") }
            : tab === "protect"
              ? { title: t("nav.protect"), body: t("protect.help") }
              : { title: t("nav.rider"), body: t("rider.help") };

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3 pt-4">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">{t("nav.studio")}</h1>
          <HelpHint title={help.title} description={help.body} />
        </div>
        <Segmented
          value={tab}
          onChange={(v) => onTab(v as StudioTab)}
          options={[
            { value: "designer", label: t("nav.designer") },
            { value: "paints", label: t("nav.paints") },
            ...(hasRider
              ? [
                  { value: "rider" as const, label: t("nav.rider") },
                  { value: "pose" as const, label: t("nav.pose") },
                ]
              : []),
            { value: "track", label: t("nav.track") },
            ...(hasLock ? [{ value: "protect" as const, label: t("nav.protect") }] : []),
          ]}
        />
      </header>

      {/* Hidden, not unmounted. These hold real work — a stack of layers, a list of sheets,
          a half-built kit — and losing it because you glanced at the tab next door is the
          kind of thing that makes people stop trusting a tool. The cost is a live WebGL
          context per visited sub-view, which `frameloop="demand"` keeps idle. */}
      {visited.has("designer") && (
        <Pane active={tab === "designer"}>
          <Designer incoming={handoff} onIncomingLoaded={() => setHandoff(null)} />
        </Pane>
      )}
      {visited.has("track") && (
        <Pane active={tab === "track"}>
          <TrackStudio />
        </Pane>
      )}
      {visited.has("protect") && hasLock && (
        <Pane active={tab === "protect"}>
          <Protect />
        </Pane>
      )}
      {visited.has("paints") && (
        <Pane active={tab === "paints"}>
          <PaintStudio
            onSendToDesigner={(sheets) => {
              setHandoff(sheets.map((s) => s.path));
              onTab("designer");
            }}
          />
        </Pane>
      )}
      {/* Rider and Pose are two views of one kit, so the kit lives above both of them and a
          preset is handed to it rather than to whichever sub-view happens to be mounted.
          Still only paid for by someone who opens one of the two. */}
      {hasRider && (visited.has("rider") || visited.has("pose")) && (
        <RiderKitProvider
          initialLoadout={riderPreset}
          initialBike={riderBike}
          onLoaded={onRiderPresetLoaded}
        >
          {visited.has("rider") && (
            <Pane active={tab === "rider"}>
              <RiderStudio />
            </Pane>
          )}
          {visited.has("pose") && (
            <Pane active={tab === "pose"}>
              <PoseStudio />
            </Pane>
          )}
        </RiderKitProvider>
      )}
    </div>
  );
}

/** A sub-view that keeps its state while another one is on screen. */
function Pane({ active, children }: { active: boolean; children: React.ReactNode }) {
  return (
    <div className={cn("min-h-0 flex-1 flex-col", active ? "flex" : "hidden")}>{children}</div>
  );
}
