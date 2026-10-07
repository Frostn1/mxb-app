import { useEffect, useState } from "react";
import { isBlockedDownload, probeDownloadSize } from "@frost/shared/api/mods";
import { formatBytes } from "@frost/shared/lib/mods";

/** What the hosts have already answered this session, by link; `null` is "won't say". */
const answered = new Map<string, number | null>();

/**
 * The size of one download, as text like "45 MB", or `""` until it is known and whenever the
 * host does not say.
 *
 * One link at a time — the one the page or dialog is about — rather than every link on a
 * grid, so browsing never fans requests out to file hosts. Links that only open in a browser
 * are never asked.
 */
export function useDownloadSize(
  option: { url: string; host: string } | null | undefined,
): string {
  const url = option?.url ?? "";
  const host = option?.host ?? "";
  const skip = !url || isBlockedDownload({ url, host });
  const [size, setSize] = useState<number | null>(() => (skip ? null : (answered.get(url) ?? null)));

  useEffect(() => {
    if (skip) {
      setSize(null);
      return;
    }
    if (answered.has(url)) {
      setSize(answered.get(url) ?? null);
      return;
    }
    let cancelled = false;
    setSize(null);
    probeDownloadSize(url, host)
      .then((n) => {
        answered.set(url, n);
        if (!cancelled) setSize(n);
      })
      .catch(() => {
        answered.set(url, null);
      });
    return () => {
      cancelled = true;
    };
  }, [url, host, skip]);

  return size ? formatBytes(size) : "";
}
