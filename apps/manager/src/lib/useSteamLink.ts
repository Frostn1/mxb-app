import { useCallback, useEffect, useRef, useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import {
  mxbsecureAutoUnlock,
  steamLinkStart,
  steamLinkStatus,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";

interface SteamLinkCallbacks {
  /** The account just got its Steam ID. */
  onLinked?: (id: string) => void;
  /** The unlock pass after a sign-in finished — re-read anything showing locked state. */
  onUnlocked?: () => void;
}

/**
 * Sign in with Steam: the control plane returns an OpenID URL, we open it in the browser, and
 * the browser half links the account. Polls until it lands (or gives up), then pulls keys for
 * anything now owned, since the identity that decides entitlement just changed.
 */
export function useSteamLink(callbacks: SteamLinkCallbacks = {}) {
  const t = useT();
  const [linking, setLinking] = useState(false);
  const cbs = useRef(callbacks);
  useEffect(() => {
    cbs.current = callbacks;
  });

  const linkSteam = useCallback(async () => {
    setLinking(true);
    try {
      const url = await steamLinkStart();
      await openUrl(url);
      toast.info(t("settings.steamLinkOpened"));
      const started = Date.now();
      while (Date.now() - started < 150_000) {
        await new Promise((r) => setTimeout(r, 2500));
        let id: string | null = null;
        try {
          id = await steamLinkStatus();
        } catch {
          // keep polling
        }
        if (id) {
          toast.success(t("settings.steamLinkOk", { id }));
          cbs.current.onLinked?.(id);
          void mxbsecureAutoUnlock(true)
            .catch(() => 0)
            .then(() => cbs.current.onUnlocked?.());
          return;
        }
      }
      toast.info(t("settings.steamLinkPending"));
    } catch (e) {
      toast.error(t("settings.steamLinkFail"), { description: String(e) });
    } finally {
      setLinking(false);
    }
  }, [t]);

  return { linking, linkSteam };
}
