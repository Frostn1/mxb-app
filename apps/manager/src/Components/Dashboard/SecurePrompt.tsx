import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { onMxbsecureBlocked } from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { useT } from "@/i18n";
import { useSteamLink } from "@/lib/useSteamLink";
import type { SectionId } from "../Settings/Settings";
import { useFrostmod } from "../../Context/FrostmodContext";

/** Reasons already told this run. Module-level so a remount doesn't repeat one. */
const told = new Set<string>();

/**
 * Says why secured mods on disk won't play, with the one step that fixes it: sign in with
 * Steam, or enroll first. The auto-unlock pass finds the reason; this only speaks, once a run
 * per reason, so a player with no secured mods never sees it.
 */
export default function SecurePrompt({
  onOpenSettings,
}: {
  onOpenSettings: (section: SectionId) => void;
}) {
  const t = useT();
  const { config } = useConfig();
  const enabled = config.mxbsecureEnabled ?? true;
  const { linkSteam } = useSteamLink();
  const { integrationChoice, requestIntegrationConsent } = useFrostmod();
  const link = useRef(linkSteam);
  useEffect(() => {
    link.current = linkSteam;
  });

  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    let off: (() => void) | undefined;
    void onMxbsecureBlocked(({ reason, count }) => {
      // Protected content truly needs the in-game component. Ask again at that point instead
      // of turning an app-only choice into a failed unlock or a surprise executable download.
      if (integrationChoice !== "enabled") {
        requestIntegrationConsent();
        return;
      }
      if (told.has(reason)) return;
      told.add(reason);
      const steam = reason === "steam";
      toast.info(t("secure.promptTitle", { count }), {
        description: t(steam ? "secure.promptSteam" : "secure.promptEnroll"),
        duration: 30_000,
        action: steam
          ? { label: t("settings.steamLinkBtn"), onClick: () => void link.current() }
          : { label: t("secure.promptEnrollBtn"), onClick: () => onOpenSettings("paintsync") },
      });
    }).then((u) => (alive ? (off = u) : u()));
    return () => {
      alive = false;
      off?.();
    };
  }, [enabled, integrationChoice, requestIntegrationConsent, t, onOpenSettings]);

  return null;
}
