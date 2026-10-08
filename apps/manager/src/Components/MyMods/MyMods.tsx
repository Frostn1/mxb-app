/**
 * My mods: the rider's uploads to the mxbsecure catalogue. Uploads in flight with their
 * progress, then every mod with its state, visibility and open reports, and what the owner
 * can do to it (`control-plane/src/uploads.ts:378` onwards).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { ExternalLink, Flag, Loader2, Pause, Pencil, Play, RotateCw, Trash2, Upload, X } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@frost/shared/Components/ui/alert-dialog";
import { formatBytes } from "@frost/shared/lib/mods";
import { steamLinkStatus } from "@frost/shared/api/mods";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { ContextBarRight } from "../Shell/ContextBar";
import { useViewActive } from "../Shell/RetainedView";
import { useSteamLink } from "../../lib/useSteamLink";
import {
  cancelUpload,
  deleteMod,
  dismissUpload,
  myMods,
  onUploadProgress,
  pauseUpload,
  resumeUpload,
  uploadJobs,
  type ModKind,
  type MyMod,
  type MyMods as MyModsData,
  type UploadJob,
  type UploadPhase,
} from "../../api/modUpload";
import {
  accountError,
  accountProblem,
  MAX_UPLOAD_BYTES,
  modPageUrl,
  progressOf,
  type AccountState,
} from "../../lib/modUpload";
import UploadDialog, { KIND_LABEL } from "./UploadDialog";
import EditModDialog from "./EditModDialog";

const PHASE_LABEL: Record<UploadPhase, TKey> = {
  hashing: "uploadPhase.hashing",
  uploading: "uploadPhase.uploading",
  paused: "uploadPhase.paused",
  completing: "uploadPhase.completing",
  checking: "uploadPhase.checking",
  live: "uploadPhase.live",
  rejected: "uploadPhase.rejected",
  failed: "uploadPhase.failed",
};

export default function MyMods({ uploadRequest }: { uploadRequest: number }) {
  const t = useT();
  const active = useViewActive();
  const [mine, setMine] = useState<MyModsData | null>(null);
  const [jobs, setJobs] = useState<UploadJob[]>([]);
  const [signedIn, setSignedIn] = useState(true);
  const [blocked, setBlocked] = useState<string | null>(null);
  const [steam, setSteam] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [uploadOpen, setUploadOpen] = useState(false);
  const [versionOf, setVersionOf] = useState<MyMod | null>(null);
  const [editing, setEditing] = useState<MyMod | null>(null);
  const [deleting, setDeleting] = useState<MyMod | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setJobs(await uploadJobs());
      setSteam(Boolean(await steamLinkStatus().catch(() => null)));
      setMine(await myMods());
      setSignedIn(true);
      setBlocked(null);
    } catch (e) {
      const acc = accountError(e);
      if (acc) {
        setSignedIn(acc.signedIn);
        setBlocked(acc.blocked);
      } else {
        setError(String(e));
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  // Progress from the Rust side. A job that just finished changes the list, so reload it.
  useEffect(() => {
    const off = onUploadProgress((job) => {
      setJobs((all) => {
        const i = all.findIndex((j) => j.key === job.key);
        const prev = i >= 0 ? all[i] : null;
        if (prev && prev.phase !== job.phase && (job.phase === "checking" || job.phase === "live" || job.phase === "rejected"))
          void myMods().then(setMine).catch(() => {});
        return i >= 0 ? all.map((j, n) => (n === i ? job : j)) : [job, ...all];
      });
    });
    return () => {
      void off.then((f) => f()).catch(() => {});
    };
  }, []);

  const { linking, linkSteam } = useSteamLink({ onLinked: () => void load() });

  const state: AccountState = { signedIn, steam, blocked, mine, jobs };
  const problem = accountProblem(state);

  const openUpload = useCallback((of: MyMod | null) => {
    setVersionOf(of);
    setUploadOpen(true);
  }, []);

  // "Upload a mod" from elsewhere in the app lands here with the form open.
  const seen = useRef(0);
  useEffect(() => {
    if (uploadRequest > seen.current) {
      seen.current = uploadRequest;
      openUpload(null);
    }
  }, [uploadRequest, openUpload]);

  const quota = mine?.quota;
  const limits = t("myMods.limits", {
    max: formatBytes(MAX_UPLOAD_BYTES),
    perDay: quota?.uploadsPerDay ?? 20,
    bytesDay: formatBytes(quota?.bytesPerDay ?? 10 * 1024 ** 3),
    stored: formatBytes(quota?.storageBytes ?? 25 * 1024 ** 3),
  });

  const act = async (run: () => Promise<void>, failKey: TKey) => {
    try {
      await run();
    } catch (e) {
      toast.error(t(failKey), { description: String(e) });
    }
    setJobs(await uploadJobs().catch(() => jobs));
  };

  const confirmDelete = async () => {
    if (!deleting) return;
    const m = deleting;
    setDeleting(null);
    try {
      await deleteMod(m.id);
      toast.success(t("myMods.deleted"));
    } catch (e) {
      toast.error(t("myMods.deleteFailed"), { description: String(e) });
    }
    await load();
  };

  const visibleJobs = jobs.slice().sort((a, b) => b.startedAt - a.startedAt);
  const checkingAssets = useMemo(
    () => new Set(jobs.filter((j) => j.phase === "checking").map((j) => j.assetId)),
    [jobs],
  );

  return (
    <div className="flex h-full flex-col">
      <ContextBarRight>
        <Button variant="outline" size="sm" onClick={() => void load()} disabled={loading}>
          <RotateCw className={cn("size-3.5", loading && "animate-spin")} /> {t("myMods.refresh")}
        </Button>
        <Button size="sm" onClick={() => openUpload(null)} disabled={problem !== null}>
          <Upload className="size-3.5" /> {t("myMods.upload")}
        </Button>
      </ContextBarRight>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        <div className="flex max-w-[860px] flex-col gap-5 pt-2">
          <p className="text-[12px] text-muted-foreground">{limits}</p>

          {problem && (
            <section className="flex items-center gap-4 rounded-xl border border-amber-400/30 bg-card p-4">
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-semibold">
                  {t(
                    problem.code === "signin"
                      ? "myMods.signinTitle"
                      : problem.code === "steam"
                        ? "myMods.steamTitle"
                        : problem.code === "blocked"
                          ? "myMods.blockedTitle"
                          : "uploadErr.openLimitTitle",
                  )}
                </div>
                <p className="mt-0.5 text-[11.5px] text-muted-foreground">
                  {problem.code === "signin"
                    ? t("myMods.signinHint")
                    : problem.code === "steam"
                      ? t("myMods.steamHint")
                      : problem.code === "blocked"
                        ? problem.message
                        : t("uploadErr.openLimit", { count: problem.limit })}
                </p>
              </div>
              {problem.code === "steam" && (
                <Button size="sm" onClick={() => void linkSteam()} disabled={linking}>
                  {linking && <Loader2 className="size-3.5 animate-spin" />}
                  {t("myMods.steamButton")}
                </Button>
              )}
            </section>
          )}

          {error && (
            <p className="select-text rounded-xl border border-destructive/30 p-3 text-[12px] text-destructive">
              {t("myMods.loadFailed")}: {error}
            </p>
          )}

          {visibleJobs.length > 0 && (
            <section className="flex flex-col gap-2">
              <h2 className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">{t("myMods.uploads")}</h2>
              {visibleJobs.map((j) => (
                <JobRow
                  key={j.key}
                  job={j}
                  onPause={() => void act(() => pauseUpload(j.key), "uploadPhase.failed")}
                  onResume={() => void act(() => resumeUpload(j.key), "myMods.resumeFailed")}
                  onCancel={() => void act(() => cancelUpload(j.key), "uploadPhase.failed")}
                  onDismiss={() => void act(() => dismissUpload(j.key), "uploadPhase.failed")}
                />
              ))}
            </section>
          )}

          <section className="flex flex-col gap-2">
            <h2 className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">{t("myMods.mods")}</h2>
            {!mine ? (
              <p className="py-10 text-center text-[13px] text-muted-foreground">
                {loading ? t("common.loading") : ""}
              </p>
            ) : mine.mods.length === 0 ? (
              <div className="rounded-xl border border-dashed border-white/[0.08] px-4 py-8 text-center">
                <div className="text-[13px] font-semibold">{t("myMods.empty")}</div>
                <p className="mt-0.5 text-[12px] text-muted-foreground">{t("myMods.emptyHint")}</p>
              </div>
            ) : (
              mine.mods.map((m) => {
                const pending = mine.uploads.find((u) => u.assetId === m.id && (u.state === "open" || u.state === "verifying"));
                const rejected = mine.uploads.find((u) => u.assetId === m.id && u.state === "rejected");
                return (
                  <ModRow
                    key={m.id}
                    mod={m}
                    checking={checkingAssets.has(m.id) || pending?.state === "verifying"}
                    rejected={m.currentVersion === null ? (rejected?.error ?? null) : null}
                    canUpload={problem === null}
                    onEdit={() => setEditing(m)}
                    onDelete={() => setDeleting(m)}
                    onNewVersion={() => openUpload(m)}
                  />
                );
              })
            )}
          </section>
        </div>
      </div>

      <UploadDialog
        open={uploadOpen && problem === null}
        onOpenChange={setUploadOpen}
        mods={mine?.mods ?? []}
        versionOf={versionOf}
        limits={limits}
        onStarted={(job) => setJobs((all) => [job, ...all.filter((j) => j.key !== job.key)])}
      />
      <EditModDialog mod={editing} onClose={() => setEditing(null)} onSaved={() => void load()} />

      <AlertDialog open={!!deleting} onOpenChange={(o) => !o && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("myMods.deleteTitle", { title: deleting?.title ?? "" })}</AlertDialogTitle>
            <AlertDialogDescription>{t("myMods.deleteBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>{t("myMods.delete")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function JobRow({
  job,
  onPause,
  onResume,
  onCancel,
  onDismiss,
}: {
  job: UploadJob;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onDismiss: () => void;
}) {
  const t = useT();
  const pct = Math.round(progressOf(job) * 100);
  const running = job.phase === "hashing" || job.phase === "uploading";
  const resumable = (job.phase === "paused" || job.phase === "failed") && job.uploadId !== null;
  const over = job.phase === "live" || job.phase === "rejected" || (job.phase === "failed" && job.uploadId === null);
  const tone =
    job.phase === "live"
      ? "text-primary"
      : job.phase === "rejected" || job.phase === "failed"
        ? "text-destructive"
        : "text-muted-foreground";
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-white/[0.07] bg-card p-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold">{job.meta.title || job.filename}</div>
          <div className="flex gap-1.5 text-[11.5px]">
            <span className={cn("font-semibold", tone)}>{t(PHASE_LABEL[job.phase])}</span>
            {(running || job.phase === "paused") && (
              <span className="tabular-figures text-muted-foreground">
                {t("uploadPhase.of", { sent: formatBytes(job.sent) || "0 B", size: formatBytes(job.size) })}
              </span>
            )}
            {job.phase === "checking" && <span className="text-muted-foreground">{t("uploadPhase.checkingHint")}</span>}
          </div>
        </div>
        {job.phase === "live" && job.assetId !== null && (
          <Button variant="outline" size="sm" onClick={() => void openUrl(modPageUrl(job.assetId!))}>
            <ExternalLink className="size-3.5" /> {t("myMods.view")}
          </Button>
        )}
        {running && (
          <Button variant="outline" size="sm" onClick={onPause}>
            <Pause className="size-3.5" /> {t("uploadPhase.pause")}
          </Button>
        )}
        {resumable && (
          <Button variant="outline" size="sm" onClick={onResume}>
            <Play className="size-3.5" /> {t("uploadPhase.resume")}
          </Button>
        )}
        {(running || resumable) && (
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
        )}
        {over && (
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onDismiss} title={t("uploadPhase.dismiss")}>
            <X className="size-4" />
          </Button>
        )}
      </div>
      {!over && (
        <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.06]">
          <div
            className={cn("h-full transition-[width]", job.phase === "checking" ? "animate-pulse bg-primary/60" : "bg-primary")}
            style={{ width: `${pct}%` }}
          />
        </div>
      )}
      {job.error && (job.phase === "rejected" || job.phase === "failed") && (
        <p className="select-text text-[11.5px] text-destructive">{job.error}</p>
      )}
    </div>
  );
}

function ModRow({
  mod,
  checking,
  rejected,
  canUpload,
  onEdit,
  onDelete,
  onNewVersion,
}: {
  mod: MyMod;
  checking: boolean;
  rejected: string | null;
  canUpload: boolean;
  onEdit: () => void;
  onDelete: () => void;
  onNewVersion: () => void;
}) {
  const t = useT();
  const live = mod.currentVersion !== null && mod.state === "active";
  const status: { key: TKey; tone: string } =
    mod.state === "removed"
      ? { key: "myMods.stateRemoved", tone: "text-destructive" }
      : mod.state === "hidden"
        ? { key: "myMods.stateHidden", tone: "text-amber-400" }
        : checking
          ? { key: "uploadPhase.checking", tone: "text-muted-foreground" }
          : rejected !== null
            ? { key: "uploadPhase.rejected", tone: "text-destructive" }
            : live
              ? { key: "uploadPhase.live", tone: "text-primary" }
              : { key: "myMods.stateNotLive", tone: "text-muted-foreground" };
  const owned = mod.state === "active" || mod.state === "hidden";
  const kind = KIND_LABEL[mod.modType as ModKind];
  return (
    <div className="flex items-center gap-3 rounded-xl border border-white/[0.07] bg-card p-3">
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-semibold">{mod.title}</div>
        <div className="flex flex-wrap items-center gap-x-1.5 text-[11.5px] text-muted-foreground">
          <span className={cn("font-semibold", status.tone)}>{t(status.key)}</span>
          <span>·</span>
          <span>{kind ? t(kind) : mod.modType}</span>
          <span>·</span>
          <span>{t(mod.visibility === "unlisted" ? "upload.unlisted" : "upload.public")}</span>
          {mod.reports > 0 && (
            <>
              <span>·</span>
              <span className="flex items-center gap-1 text-amber-400">
                <Flag className="size-3" /> {t("myMods.reports", { count: mod.reports })}
              </span>
            </>
          )}
        </div>
        {rejected && <p className="select-text text-[11.5px] text-destructive">{rejected}</p>}
      </div>
      {live && (
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => void openUrl(modPageUrl(mod.id))} title={t("myMods.view")}>
          <ExternalLink className="size-4" />
        </Button>
      )}
      {owned && (
        <>
          <Button variant="outline" size="sm" onClick={onNewVersion} disabled={!canUpload}>
            <Upload className="size-3.5" /> {t("myMods.newVersion")}
          </Button>
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onEdit} title={t("myMods.edit")}>
            <Pencil className="size-4" />
          </Button>
        </>
      )}
      {mod.state !== "removed" && (
        <Button variant="ghost" size="icon" className="h-8 w-8 text-destructive" onClick={onDelete} title={t("myMods.delete")}>
          <Trash2 className="size-4" />
        </Button>
      )}
    </div>
  );
}
