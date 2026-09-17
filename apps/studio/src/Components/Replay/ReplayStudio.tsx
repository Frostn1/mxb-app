import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  CircleDot,
  Clapperboard,
  Download,
  FolderOpen,
  Loader2,
  RefreshCw,
  Square,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Switch } from "@frost/shared/Components/ui/switch";
import { cn } from "@frost/shared/lib/utils";
import { revealInExplorer } from "@frost/shared/api/mods";
import {
  deleteRecording,
  fetchFfmpeg,
  replayCheck,
  replayOutDir,
  replayRecord,
  replayRecordings,
  replaySettings,
  replaySlots,
  replayStop,
  saveReplaySettings,
  REPLAY_EVENT,
  type Encoder,
  type Quality,
  type Recorded,
  type RecordingSettings,
  type ReplayStatus,
  type Slot,
} from "@frost/shared/api/replay";
import { useT } from "@/i18n";
import { track } from "@/lib/analytics";

/**
 * Replay — the Replay Mod's half that lives outside the game.
 *
 * The mod flies the camera; this screen keeps what it flew. It is in the Studio and not in
 * the mod manager because cutting a replay is the same errand as painting a bike or building
 * a track — you are making something — and none of it is managing mods.
 *
 * The screen is deliberately quiet. Recording happens whether or not this window is open: the
 * watcher behind it starts on the mod's own signal, so what is drawn here is a status, a
 * settings panel and a list of files — not a control anybody has to reach for mid-shot.
 */
export default function ReplayStudio() {
  const t = useT();
  const [status, setStatus] = useState<ReplayStatus | null>(null);
  const [slots, setSlots] = useState<Slot[]>([]);
  const [files, setFiles] = useState<Recorded[]>([]);
  const [settings, setSettings] = useState<RecordingSettings | null>(null);
  const [dir, setDir] = useState("");
  const [fetching, setFetching] = useState(false);
  // The recordings list is re-read when a recording ends. Held in a ref so the listener does
  // not have to be torn down and rebuilt every time the status changes.
  const wasRecording = useRef(false);

  const reloadFiles = useCallback(() => {
    replayRecordings().then(setFiles).catch(() => setFiles([]));
  }, []);

  useEffect(() => {
    track("view.studio.replay");
    // `replayCheck` rather than `replayStatus`: opening the screen is the moment to find out
    // what this machine's ffmpeg can actually do, and the only moment worth paying for it.
    replayCheck().then(setStatus).catch(() => {});
    replaySettings().then(setSettings).catch(() => {});
    replaySlots().then(setSlots).catch(() => {});
    replayOutDir().then(setDir).catch(() => {});
    reloadFiles();
  }, [reloadFiles]);

  useEffect(() => {
    const pending = listen<ReplayStatus>(REPLAY_EVENT, (e) => {
      setStatus(e.payload);
      // A recording that has just stopped has a file on disk that the list does not know
      // about yet — and the moment somebody wants to see it is this one.
      if (wasRecording.current && !e.payload.recording) {
        reloadFiles();
        replaySlots().then(setSlots).catch(() => {});
      }
      wasRecording.current = e.payload.recording;
    });
    return () => void pending.then((off) => off()).catch(() => {});
  }, [reloadFiles]);

  const save = useCallback(async (next: RecordingSettings) => {
    setSettings(next);
    try {
      const stored = await saveReplaySettings(next);
      setSettings(stored);
      setStatus(await replayCheck());
      setDir(await replayOutDir());
    } catch (e) {
      toast.error(String(e));
    }
  }, []);

  const record = async () => {
    try {
      if (status?.recording) await replayStop();
      else await replayRecord();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const getFfmpeg = async () => {
    setFetching(true);
    try {
      await fetchFfmpeg();
      setStatus(await replayCheck());
      toast.success(t("replay.ffmpegReady"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setFetching(false);
    }
  };

  return (
    <div className="flex h-full min-h-0">
      <section className="flex min-w-0 flex-1 flex-col border-r border-border">
        <Head label={t("replay.title")}>
          <Button
            size="sm"
            variant={status?.recording ? "outline" : "default"}
            className="ml-auto"
            onClick={() => void record()}
          >
            {status?.recording ? (
              <>
                <Square className="size-3.5" /> {t("replay.stop")}
              </>
            ) : (
              <>
                <CircleDot className="size-3.5" /> {t("replay.recordNow")}
              </>
            )}
          </Button>
        </Head>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
          <StatusCard status={status} />

          <h3 className="mt-6 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("replay.recordings")}
          </h3>
          {files.length === 0 ? (
            <p className="mt-2 border border-dashed border-border px-4 py-8 text-center text-[12.5px] text-muted-foreground">
              {t("replay.noRecordings")}
            </p>
          ) : (
            <ul className="mt-2 border border-border bg-card text-[12px]">
              {files.map((f) => (
                <li
                  key={f.path}
                  className="flex items-center gap-3 border-b border-border/60 px-3 py-2 last:border-b-0"
                >
                  <Clapperboard className="size-3.5 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate" title={f.name}>
                      {f.name}
                    </div>
                    <div className="text-[11px] text-muted-foreground">
                      {megabytes(f.bytes)} · {when(f.modified)}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    title={t("replay.reveal")}
                    onClick={() => void revealInExplorer(f.path)}
                  >
                    <FolderOpen className="size-3.5" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    title={t("common.delete")}
                    onClick={() => {
                      void deleteRecording(f.path)
                        .then(reloadFiles)
                        .catch((e) => toast.error(String(e)));
                    }}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </li>
              ))}
            </ul>
          )}

          <div className="mt-3 flex items-center gap-2">
            <Button size="sm" variant="outline" onClick={() => void revealInExplorer(dir)}>
              <FolderOpen className="size-3.5" /> {t("replay.openFolder")}
            </Button>
            <Button size="sm" variant="ghost" onClick={reloadFiles}>
              <RefreshCw className="size-3.5" />
            </Button>
            <span className="truncate font-mono text-[11px] text-muted-foreground" title={dir}>
              {dir}
            </span>
          </div>
        </div>
      </section>

      <section className="flex w-[380px] flex-none flex-col">
        <Head label={t("replay.setup")} />
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
          {settings === null ? (
            <Loader2 className="size-4 animate-spin text-muted-foreground" />
          ) : (
            <Setup
              settings={settings}
              status={status}
              fetching={fetching}
              onSave={(s) => void save(s)}
              onFetch={() => void getFfmpeg()}
            />
          )}

          <h3 className="mt-7 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("replay.paths")}
          </h3>
          <p className="mt-1.5 text-[11.5px] leading-relaxed text-muted-foreground">
            {t("replay.pathsDesc")}
          </p>
          {slots.length === 0 ? (
            <p className="mt-2 border border-dashed border-border px-4 py-6 text-center text-[12.5px] text-muted-foreground">
              {t("replay.noPaths")}
            </p>
          ) : (
            <ul className="mt-2 border border-border bg-card text-[12px]">
              {slots.map((s) => (
                <li
                  key={s.path}
                  className="flex items-center justify-between gap-3 border-b border-border/60 px-3 py-1.5 last:border-b-0"
                >
                  <span className="truncate" title={s.path}>
                    {s.slot === null ? s.name : t("replay.slotN", { n: String(s.slot) })}
                  </span>
                  <span className="flex-none text-[11px] text-muted-foreground">
                    {when(s.modified)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>
    </div>
  );
}

function Head({ label, children }: { label: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-none items-center gap-2 px-6 pb-2.5 pt-4">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{label}</h2>
      {children}
    </div>
  );
}

/**
 * What is happening right now, in one sentence.
 *
 * The states are ordered by what the person can do about them, not by severity: a recording
 * in flight is the only thing worth a headline, and everything below it is a different
 * missing piece — no encoder, no mod, no game, or nothing playing yet.
 */
function StatusCard({ status }: { status: ReplayStatus | null }) {
  const t = useT();
  if (!status) {
    return <Loader2 className="size-4 animate-spin text-muted-foreground" />;
  }

  const line = (() => {
    if (status.recording) {
      return {
        tone: "live" as const,
        title: t("replay.recording", { time: clock(status.seconds) }),
        detail:
          status.source === "auto" ? t("replay.byTheMod") : t("replay.byHand"),
      };
    }
    if (!status.ffmpeg) {
      return { tone: "warn" as const, title: t("replay.noFfmpeg"), detail: t("replay.noFfmpegDetail") };
    }
    if (!status.modSeen) {
      return { tone: "idle" as const, title: t("replay.noMod"), detail: t("replay.noModDetail") };
    }
    if (!status.gameRunning) {
      return { tone: "idle" as const, title: t("replay.gameClosed"), detail: t("replay.gameClosedDetail") };
    }
    return { tone: "idle" as const, title: t("replay.waiting"), detail: t("replay.waitingDetail") };
  })();

  // The one failure that produces a file rather than an error: a capture that cannot see a
  // game which has taken the screen. Said before the recording, since afterwards it is a
  // black video and an evening lost.
  const blackRisk = status.ddagrab === false && status.exclusiveFullscreen;

  return (
    <div className="border border-border bg-card px-4 py-3">
      <div className="flex items-baseline gap-2">
        <span
          className={cn(
            "size-2 shrink-0 rounded-full",
            line.tone === "live" && "animate-pulse bg-red-500",
            line.tone === "warn" && "bg-amber-500",
            line.tone === "idle" && "bg-muted-foreground/50",
          )}
        />
        <span className="text-[13px] font-medium">{line.title}</span>
      </div>
      <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">{line.detail}</p>
      {status.file && (
        <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground" title={status.file}>
          {status.file}
        </p>
      )}
      {blackRisk && (
        <p className="mt-2 text-[11.5px] leading-relaxed text-amber-600 dark:text-amber-500">
          {t("replay.fullscreenWarning")}
        </p>
      )}
      {status.error && (
        <p className="mt-2 text-[11.5px] leading-relaxed text-amber-600 dark:text-amber-500">
          {status.error}
        </p>
      )}
    </div>
  );
}

/** The settings panel: what to record with, and where it goes. */
function Setup({
  settings,
  status,
  fetching,
  onSave,
  onFetch,
}: {
  settings: RecordingSettings;
  status: ReplayStatus | null;
  fetching: boolean;
  onSave: (s: RecordingSettings) => void;
  onFetch: () => void;
}) {
  const t = useT();
  const set = <K extends keyof RecordingSettings>(key: K, value: RecordingSettings[K]) =>
    onSave({ ...settings, [key]: value });

  return (
    <div className="space-y-4">
      <label className="flex items-start justify-between gap-3">
        <span className="min-w-0">
          <span className="text-[12.5px] font-medium">{t("replay.auto")}</span>
          <span className="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
            {t("replay.autoDesc")}
          </span>
        </span>
        <Switch checked={settings.auto} onCheckedChange={(v) => set("auto", v)} />
      </label>

      <Field label={t("replay.quality")}>
        <Segmented<Quality>
          size="sm"
          value={settings.quality}
          onChange={(v) => set("quality", v)}
          options={[
            { value: "high", label: t("replay.qualityHigh") },
            { value: "balanced", label: t("replay.qualityBalanced") },
            { value: "small", label: t("replay.qualitySmall") },
          ]}
        />
      </Field>

      <Field label={t("replay.fps")}>
        <Segmented<string>
          size="sm"
          value={String(settings.fps)}
          onChange={(v) => set("fps", Number(v))}
          options={[
            { value: "30", label: "30" },
            { value: "60", label: "60" },
          ]}
        />
      </Field>

      <Field label={t("replay.encoder")} hint={t("replay.encoderDesc")}>
        <Segmented<Encoder>
          size="sm"
          value={settings.encoder}
          onChange={(v) => set("encoder", v)}
          options={[
            { value: "auto", label: t("replay.encoderAuto") },
            { value: "nvenc", label: "NVIDIA" },
            { value: "amf", label: "AMD" },
            { value: "qsv", label: "Intel" },
            { value: "x264", label: "CPU" },
          ]}
        />
      </Field>

      <Field label={t("replay.folder")}>
        <div className="flex gap-2">
          <TextSetting
            value={settings.dir}
            placeholder={t("replay.folderDefault")}
            mono
            onCommit={(v) => set("dir", v)}
          />
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              void openDialog({ directory: true }).then((picked) => {
                if (typeof picked === "string") set("dir", picked);
              });
            }}
          >
            <FolderOpen className="size-3.5" />
          </Button>
        </div>
      </Field>

      <Field label={t("replay.audio")} hint={t("replay.audioDesc")}>
        <TextSetting
          value={settings.audioDevice}
          placeholder={t("replay.audioSilent")}
          onCommit={(v) => set("audioDevice", v)}
        />
      </Field>

      <Field label={t("replay.hotkey")} hint={t("replay.hotkeyDesc")}>
        <TextSetting value={settings.hotkey} mono onCommit={(v) => set("hotkey", v)} />
      </Field>

      <div className="border-t border-border pt-3">
        <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("replay.encoderBinary")}
        </div>
        <p
          className="mt-1 truncate font-mono text-[11px] text-muted-foreground"
          title={status?.ffmpeg ?? ""}
        >
          {status?.ffmpeg ?? t("replay.ffmpegMissing")}
        </p>
        {!status?.ffmpeg && (
          <Button size="sm" className="mt-2" disabled={fetching} onClick={onFetch}>
            {fetching ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Download className="size-3.5" />
            )}
            {t("replay.getFfmpeg")}
          </Button>
        )}
      </div>
    </div>
  );
}

/**
 * A text setting that is saved when you have finished typing it.
 *
 * The switches and segmented controls here save on every change, which is right for them: one
 * click, one decision. A folder path is thirty keystrokes, and saving each one would rewrite
 * the config thirty times and re-probe ffmpeg with it. So this holds the draft and commits on
 * blur or Enter, and follows the stored value again whenever that changes underneath it —
 * which is what happens when the folder is picked from the dialog beside it.
 */
function TextSetting({
  value,
  placeholder,
  mono,
  onCommit,
}: {
  value: string;
  placeholder?: string;
  mono?: boolean;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <Input
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") setDraft(value);
      }}
      className={cn("text-[11.5px]", mono && "font-mono")}
    />
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
        {label}
      </div>
      {hint && <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">{hint}</p>}
      <div className="mt-1.5">{children}</div>
    </div>
  );
}

/** `93` -> `1:33`. */
function clock(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function megabytes(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb.toFixed(0)} MB`;
}

/** The local date and time, short. A recording is found by when it was taken. */
function when(ms: number): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
