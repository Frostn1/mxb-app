import { useEffect, useRef, useState } from "react";
import { Progress } from "@frost/shared/Components/ui/progress";
import { onInstallProgress } from "@frost/shared/api/mods";
import type { BundleProgress, InstallProgress } from "@frost/shared/types";
import { useT } from "@/i18n";

/** The slugs share imports download under — `fileshare::SLUG` and `bundle::BUNDLE_SLUG`. */
export const FILE_SHARE_SLUG = "__file_share__";
export const PRESET_BUNDLE_SLUG = "__preset_bundle__";

/** No estimate before this: the first seconds are connection setup, and a pace taken from
 *  them swings wildly. */
const ESTIMATE_AFTER_MS = 3000;

/**
 * Seconds to go at the pace since the bar started moving. Starts over whenever the bar drops
 * back — an upload recut or a retried part sends those bytes again.
 */
function useSecsLeft(fraction: number): number | undefined {
  const origin = useRef<{ at: number; from: number; last: number } | null>(null);
  const now = Date.now();
  const o = origin.current;
  if (!o || fraction < o.last) {
    origin.current = { at: now, from: fraction, last: fraction };
    return undefined;
  }
  o.last = fraction;
  const ms = now - o.at;
  const gained = fraction - o.from;
  if (ms < ESTIMATE_AFTER_MS || gained <= 0 || fraction >= 1) return undefined;
  return ((ms / 1000) * (1 - fraction)) / gained;
}

function TransferBar({ fraction, detail }: { fraction: number; detail?: string }) {
  const t = useT();
  const secs = useSecsLeft(fraction);
  const pct = fraction * 100;
  const left =
    secs === undefined
      ? null
      : secs < 60
        ? t("share.secondsLeft", { n: Math.max(5, Math.ceil(secs / 5) * 5) })
        : t("share.minutesLeft", { n: Math.ceil(secs / 60) });
  return (
    <div className="flex flex-col gap-1">
      <Progress value={pct} className="h-1.5 rounded-full" barClassName="rounded-full" />
      <div className="flex justify-between gap-2 text-[11px] text-muted-foreground">
        <span>{[detail, left].filter(Boolean).join(" · ")}</span>
        <span>{Math.round(pct)}%</span>
      </div>
    </div>
  );
}

/** How far a share upload has got — the fraction comes from the backend, which sees the
 *  bytes go out and the parts land (upload.rs). */
export function UploadBar({ progress }: { progress: BundleProgress | null }) {
  const t = useT();
  if (progress?.phase !== "uploading" || progress.fraction === undefined) return null;
  const { done = 0, total = 1, fraction } = progress;
  return (
    <TransferBar
      fraction={fraction}
      detail={total > 1 ? t("share.partsUploaded", { done, total }) : undefined}
    />
  );
}

/** A share import's download, heard on `install-progress` under its `slug`. */
export function DownloadBar({ slug }: { slug: string }) {
  const [p, setP] = useState<InstallProgress | null>(null);
  useEffect(() => {
    let off: (() => void) | undefined;
    let gone = false;
    void onInstallProgress((e) => {
      if (e.slug === slug && e.stage === "downloading") setP(e);
    }).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off?.();
    };
  }, [slug]);
  if (!p?.total || p.received === undefined) return null;
  return <TransferBar fraction={Math.min(1, p.received / p.total)} />;
}
