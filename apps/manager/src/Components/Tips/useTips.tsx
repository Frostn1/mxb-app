import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { setTexcompress, texcompressState, type TexCompress } from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { usePlatform } from "@frost/shared/lib/usePlatform";
import { useT, type TKey } from "@/i18n";
import { useFrostmod } from "../../Context/FrostmodContext";
import { versionAtLeast } from "../../lib/tips";
import { usePaintSyncSetup } from "../../lib/usePaintSyncSetup";
import PaintSyncConfirm from "../PaintSync/PaintSyncConfirm";
import { useTipNav } from "./TipNav";

/** Where MXB Coach is described and downloaded. The same page the README links. */
const COACH_URL = "https://mxbsecure.com/coach";

/** FrostMod releases that brought the features two tips point at. */
const F8_REFRESH_VERSION = "0.49.8";
const TEXCOMPRESS_VERSION = "0.49.9";

export interface Tip {
  id: string;
  title: TKey;
  body: TKey;
  /** Whether it applies to this player right now; `null` while that's still being asked. */
  eligible: boolean | null;
  action?: { label: TKey; run: () => void };
}

/**
 * Every tip, in the order they're offered, each with its condition and action.
 *
 * Only real features that are easy to miss, and only for someone not already using them.
 * Returns the dialogs the actions open too, for the caller to render: the paint sync tip's
 * action is the same confirm the Settings card uses, listing what it will switch on.
 */
export function useTips(): { tips: Tip[]; dialogs: ReactNode } {
  const t = useT();
  const { game } = useConfig();
  const platform = usePlatform();
  const { integrationChoice, status: frostmod } = useFrostmod();
  const { navigate } = useTipNav();
  const paintSync = usePaintSyncSetup();
  const [confirming, setConfirming] = useState(false);

  const integrationOn = integrationChoice === "enabled" && Boolean(frostmod?.installed);
  const frostmodVersion = frostmod?.version ?? null;
  const texcompressCapable = integrationOn && versionAtLeast(frostmodVersion, TEXCOMPRESS_VERSION);

  // Compressed textures live in FrostMod's own cfg, so ask only when a FrostMod that knows
  // the setting is installed. `undefined` = not asked yet, `null` = couldn't tell.
  const [tex, setTex] = useState<TexCompress | null | undefined>(undefined);
  useEffect(() => {
    if (!texcompressCapable) return;
    let alive = true;
    texcompressState()
      .then((s) => alive && setTex(s))
      .catch(() => alive && setTex(null));
    return () => {
      alive = false;
    };
  }, [texcompressCapable, frostmodVersion]);

  // Still waiting on FrostMod's status when the player opted in: the tips that depend on it
  // hold their place rather than being skipped for good.
  const frostmodPending = integrationChoice === "enabled" && frostmod === null;

  const tips: Tip[] = [
    {
      id: "paint-sync",
      title: "tips.paintSyncTitle",
      body: "tips.paintSyncBody",
      eligible:
        paintSync.status === null
          ? null
          : paintSync.status === "off" || paintSync.status === "needsIntegration",
      action: { label: "tips.turnItOn", run: () => setConfirming(true) },
    },
    {
      id: "storage",
      title: "tips.storageTitle",
      body: "tips.storageBody",
      eligible: true,
      action: { label: "tips.openStorage", run: () => navigate("storage") },
    },
    {
      id: "compressed-textures",
      title: "tips.texCompressTitle",
      body: "tips.texCompressBody",
      eligible: frostmodPending
        ? null
        : !texcompressCapable
          ? false
          : tex === undefined
            ? null
            : Boolean(tex?.supported && !tex.enabled),
      action: {
        label: "tips.turnItOn",
        run: () => {
          setTexcompress(true)
            .then(() => {
              setTex((s) => (s ? { ...s, enabled: true } : s));
              toast.success(t("tips.texCompressOn"), { description: t("tips.texCompressOnDesc") });
            })
            .catch((e) => toast.error(t("settings.updateFailed"), { description: String(e) }));
        },
      },
    },
    {
      id: "f8-refresh",
      title: "tips.f8Title",
      body: "tips.f8Body",
      eligible: frostmodPending
        ? null
        : integrationOn && versionAtLeast(frostmodVersion, F8_REFRESH_VERSION),
    },
    {
      id: "coach",
      title: "tips.coachTitle",
      body: "tips.coachBody",
      // Coach is a Windows app for MX Bikes, the title with Game Integration.
      eligible: platform === null ? null : platform === "windows" && Boolean(game.caps.frostmod),
      action: { label: "tips.learnMore", run: () => void openUrl(COACH_URL) },
    },
  ];

  const dialogs = (
    <PaintSyncConfirm open={confirming} onOpenChange={setConfirming} setup={paintSync} />
  );

  return { tips, dialogs };
}
