import { useCallback, useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { setSeenVersion } from "../../api/mods";
import { useConfig } from "../../Context/Config";
import { RELEASES, releaseToShow, type Release } from "./releases";

/**
 * Decides whether this launch owes the player a release showcase, and remembers the
 * answer once they've seen it.
 *
 * The durable record is `seenVersion` in the saved config, not `localStorage` — the
 * same reasoning as the intro flags (`welcomeSeen`): WebView2 loses that storage on an
 * app-data wipe, and a showcase that reappears every few launches is worse than one
 * that never appears at all.
 */
export function useReleaseShowcase() {
  const { config, reloadConfig } = useConfig();
  const [version, setVersion] = useState("");
  // Closes the modal on the click rather than on the config round-trip.
  const [dismissed, setDismissed] = useState(false);
  // Set by the "What's new" button in Settings → About, which shows the newest
  // showcase whether or not it's already been seen.
  const [replay, setReplay] = useState(false);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(""));
  }, []);

  const release = useMemo((): Release | null => {
    if (!version) return null;
    if (replay) return releaseToShow(version, "") ?? RELEASES[0] ?? null;
    if (dismissed) return null;
    return releaseToShow(version, config.seenVersion);
  }, [version, replay, dismissed, config.seenVersion]);

  const dismiss = useCallback(() => {
    setReplay(false);
    setDismissed(true);
    if (!version) return;
    // Stamps the *running* version, not the release being shown: what's been seen is
    // everything up to here, so a later launch on the same build stays quiet.
    void setSeenVersion(version)
      .then(reloadConfig)
      .catch(() => {});
  }, [version, reloadConfig]);

  return { release, dismiss, replay: () => setReplay(true) };
}
