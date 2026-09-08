import { useEffect, useState } from "react";
import { appPlatform } from "../api/mods";

/**
 * The OS the app is running on, or `null` until the backend answers.
 *
 * Callers gate Windows-only features on `platform === "windows"`, so the brief `null`
 * renders as "not Windows" — hiding a control for a moment is harmless, offering one that
 * cannot work is not.
 */
export function usePlatform(): string | null {
  const [platform, setPlatform] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    appPlatform()
      .then((p) => !cancelled && setPlatform(p))
      .catch(() => !cancelled && setPlatform(null));
    return () => {
      cancelled = true;
    };
  }, []);
  return platform;
}
