import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { onMxbsecureBlocked, onMxbsecureRevoked } from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { useT } from "@/i18n";
import { useSteamLink } from "@/lib/useSteamLink";
import type { SectionId } from "../Settings/Settings";

/** Reasons already told this run. Module-level so a remount doesn't repeat one. */
const told = new Set<string>();

/**
 * Says why secured mods on disk won't play, with the one step that fixes it: sign in with
 * Steam, or enroll first. The auto-unlock pass finds the reason; this only speaks, once a run
 * per reason, so a player with no secured mods never sees it.
 *
 * It also says when access is taken *away* — the creator removed you as a buyer, so the app
 * deleted the key. That one is not a prompt with a fix; it is the explanation for content
 * disappearing from the game, which is otherwise indistinguishable from the app being broken.
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
  const link = useRef(linkSteam);
  useEffect(() => {
    link.current = linkSteam;
  });

  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    let off: (() => void) | undefined;
    void onMxbsecureBlocked(({ reason, count }) => {
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
  }, [enabled, t, onOpenSettings]);

  // Access removed. No `told` guard and no action button: each one names different content, and
  // there is nothing the player can do here — only the creator can put it back. It stays up
  // longer than the prompts because it is the only account of why a track stopped appearing.
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    let off: (() => void) | undefined;
    void onMxbsecureRevoked(({ names }) => {
      if (!names.length) return;
      toast.warning(t("secure.revokedTitle", { count: names.length }), {
        description: t("secure.revokedBody", { names: names.join(", ") }),
        duration: 30_000,
      });
    }).then((u) => (alive ? (off = u) : u()));
    return () => {
      alive = false;
      off?.();
    };
  }, [enabled, t]);

  return null;
}
