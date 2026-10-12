import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Check, Copy, Download, FolderOpen, Loader2, RefreshCw, Share2 } from "lucide-react";
import { save as pickSavePath } from "@tauri-apps/plugin-dialog";
import {
  exportLogs,
  onLogsShareProgress,
  shareLogs,
  type LogGroup,
  type LogsShare,
} from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { Progress } from "@frost/shared/Components/ui/progress";
import { cn } from "@frost/shared/lib/utils";
import { getLocale } from "@frost/shared/i18n/core";
import { formatBytes, formatDateShort } from "@frost/shared/lib/mods";
import type { BundleProgress } from "@frost/shared/types";
import { useT } from "@/i18n";
import { logsInfo, openLogsFolder, type CoachLogsInfo, type LogsKind } from "@/api/coach";
import { copyText } from "@/lib/clipboard";
import { Section } from "./parts";

/* MXB App's Settings -> Logs (apps/manager Settings.tsx: LogRow, shareLinkText, UploadBar),
   so the two apps' Logs read the same. Kept in step with those by hand: change both or neither. */

/** "today at 14:32" / "Aug 11 at 14:32" for a log's mtime. */
function formatWhen(ms: number | null): string {
  if (!ms) return "";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  const time = d.toLocaleTimeString(getLocale(), { hour: "2-digit", minute: "2-digit" });
  const sameDay = new Date().toDateString() === d.toDateString();
  return sameDay ? time : `${formatDateShort(d.toISOString())} ${time}`;
}

/** One link is the url; a sliced bundle needs every part, in order, so it is a numbered list. */
function shareLinkText(share: LogsShare): string {
  if (share.parts.length < 2) return share.url;
  return share.parts.map((url, i) => `${i + 1}/${share.parts.length} ${url}`).join("\n");
}

/** No estimate before this: the first seconds are connection setup. */
const ESTIMATE_AFTER_MS = 3000;

/** Seconds to go at the pace since the bar started moving; starts over when the bar drops back. */
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

function UploadBar({ progress }: { progress: BundleProgress | null }) {
  const t = useT();
  const fraction = progress?.phase === "uploading" ? progress.fraction : undefined;
  const secs = useSecsLeft(fraction ?? 0);
  if (!progress || fraction === undefined) return null;
  const { done = 0, total = 1 } = progress;
  const detail = total > 1 ? t("share.partsUploaded", { done, total }) : undefined;
  const left =
    secs === undefined
      ? null
      : secs < 60
        ? t("share.secondsLeft", { n: Math.max(5, Math.ceil(secs / 5) * 5) })
        : t("share.minutesLeft", { n: Math.ceil(secs / 60) });
  const pct = fraction * 100;
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

/** One log location: where it is, what's in it, and a way into the folder. */
function LogRow({
  label,
  hint,
  group,
  onOpen,
}: {
  label: string;
  hint: string;
  /** Undefined until the backend answers. */
  group?: LogGroup;
  onOpen: () => void;
}) {
  const t = useT();
  const newest = group?.files[0];
  const status = !group
    ? t("logs.loading")
    : !group.exists
      ? t("logs.folderMissing")
      : group.files.length === 0
        ? t("logs.empty")
        : t("logs.summary", {
            count: group.files.length,
            size: formatBytes(group.files.reduce((sum, f) => sum + f.bytes, 0)),
            when: formatWhen(newest?.modified ?? null),
          });

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <span className="text-[12.5px] font-semibold text-foreground/85">{label}</span>
        <span className="text-[11.5px] text-muted-foreground">{hint}</span>
      </div>
      <div className="flex gap-2">
        <div className="flex min-w-0 flex-1 items-center rounded-lg bg-background px-3 py-2 font-mono text-[12px] text-muted-foreground">
          <span className="flex-1 truncate" title={group?.dir || undefined}>
            {group?.dir || t("logs.notSet")}
          </span>
        </div>
        <Button variant="outline" size="sm" onClick={onOpen} disabled={!group?.exists}>
          <FolderOpen className="size-3.5" /> {t("logs.open")}
        </Button>
      </div>
      <span className={cn("text-[11.5px]", group && !group.exists ? "text-warning" : "text-muted-foreground")}>
        {status}
      </span>
    </div>
  );
}

/** Where the logs are, and save or share them as one zip for a bug report. */
export default function LogsSettings() {
  const t = useT();
  // Re-read whenever the section is opened, not once on mount: the reason anyone comes here
  // is that something just went wrong.
  const [logs, setLogs] = useState<CoachLogsInfo | null>(null);
  const [exporting, setExporting] = useState(false);
  // The link the last share came back with, kept on screen: it lands on the clipboard by
  // itself, and a clipboard used for something else since is the one way a bundle is lost.
  const [shared, setShared] = useState<LogsShare | null>(null);
  const [sharing, setSharing] = useState<string | null>(null);
  const [progress, setProgress] = useState<BundleProgress | null>(null);
  const [copied, setCopied] = useState(false);

  const refresh = useCallback(() => {
    logsInfo()
      .then(setLogs)
      .catch(() => setLogs(null));
  }, []);
  useEffect(() => refresh(), [refresh]);

  const open = (which: LogsKind) => {
    openLogsFolder(which).catch((e) => toast.error(String(e)));
  };

  const save = async () => {
    const stamp = new Date().toISOString().slice(0, 10);
    const dest = await pickSavePath({
      defaultPath: `mxb-coach-logs-${stamp}.zip`,
      filters: [{ name: "Zip", extensions: ["zip"] }],
    }).catch(() => null);
    if (!dest) return;
    setExporting(true);
    try {
      const written = await exportLogs(dest);
      toast.success(t("logs.saved"), {
        description: t("logs.savedDesc", { count: written.files, size: formatBytes(written.bytes) }),
      });
      refresh();
    } catch (e) {
      toast.error(t("logs.saveFailed"), { description: String(e) });
    } finally {
      setExporting(false);
    }
  };

  const share = async () => {
    setSharing(t("logs.sharePacking"));
    setShared(null);
    setCopied(false);
    const unlisten = await onLogsShareProgress((p) => {
      if (p.phase === "done") return;
      setProgress(p);
      setSharing(p.phase === "uploading" ? p.message || t("logs.sharing") : t("logs.sharePacking"));
    });
    try {
      const result = await shareLogs();
      setShared(result);
      const ok = await copyText(shareLinkText(result));
      setCopied(ok);
      toast.success(t("logs.shared"), {
        description: ok
          ? t("logs.sharedCopied", { size: formatBytes(result.size) })
          : t("logs.sharedDesc", { size: formatBytes(result.size) }),
      });
      refresh();
    } catch (e) {
      toast.error(t("logs.shareFailed"), { description: String(e).replace(/^Error:\s*/, "") });
    } finally {
      unlisten();
      setSharing(null);
      setProgress(null);
    }
  };

  const copyLink = async () => {
    if (!shared) return;
    if (await copyText(shareLinkText(shared))) {
      setCopied(true);
      toast.success(t("logs.linkCopied"));
    }
  };

  const busy = exporting || !!sharing;
  return (
    <Section title={t("coachSettings.logs")} desc={t("logs.desc")}>
      <LogRow label={t("logs.appLogs")} hint={t("logs.appLogsDesc")} group={logs?.app} onOpen={() => open("app")} />
      <LogRow
        label={t("coachSettings.plugin")}
        hint={t("logs.recorderLogsDesc")}
        group={logs?.recorder}
        onOpen={() => open("recorder")}
      />
      <LogRow
        label={t("logs.pluginsLabel")}
        hint={t("logs.pluginsLogsDesc")}
        group={logs?.plugins}
        onOpen={() => open("plugins")}
      />
      <LogRow label={t("coachSettings.game")} hint={t("logs.gameLogsDesc")} group={logs?.game} onOpen={() => open("game")} />
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" onClick={save} disabled={busy}>
          <Download className="size-3.5" />
          {exporting ? t("logs.saving") : t("logs.save")}
        </Button>
        <Button variant="outline" size="sm" onClick={share} disabled={busy}>
          {sharing ? <Loader2 className="size-3.5 animate-spin" /> : <Share2 className="size-3.5" />}
          {sharing || t("logs.share")}
        </Button>
        <Button variant="outline" size="sm" onClick={refresh} disabled={busy}>
          <RefreshCw className="size-3.5" /> {t("logs.refresh")}
        </Button>
      </div>
      {sharing && <UploadBar progress={progress} />}
      {shared && (
        <div className="flex flex-col gap-1.5">
          <div className="flex gap-2">
            {shared.parts.length > 1 ? (
              <textarea
                readOnly
                value={shareLinkText(shared)}
                onFocus={(e) => e.currentTarget.select()}
                className="h-20 min-w-0 flex-1 resize-none rounded-lg border border-input bg-background px-3 py-2 font-mono text-[12px] leading-snug text-muted-foreground"
              />
            ) : (
              <input
                readOnly
                value={shared.url}
                onFocus={(e) => e.currentTarget.select()}
                className="min-w-0 flex-1 rounded-lg border border-input bg-background px-3 py-2 font-mono text-[12px] text-muted-foreground"
              />
            )}
            <Button variant="outline" size="sm" onClick={copyLink}>
              {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              {copied ? t("logs.linkCopiedShort") : t("logs.copyLink")}
            </Button>
          </div>
          <span className="text-[11.5px] text-muted-foreground">
            {t("logs.sharedSummary", { count: shared.files, size: formatBytes(shared.size) })}
          </span>
          <span className="text-[11.5px] text-warning">{t("logs.shareWarning")}</span>
        </div>
      )}
      <p className="text-[11.5px] leading-relaxed text-faint">{t("logs.privacy")}</p>
    </Section>
  );
}
