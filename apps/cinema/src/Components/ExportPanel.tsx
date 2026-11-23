import { useEffect, useRef, useState } from "react";
import { Ban, Check, Clapperboard, FolderOpen, Loader2 } from "lucide-react";
import { toast } from "sonner";
import type { ViewerCamera } from "@frost/shared/Components/Viewer/TrackViewer";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Slider } from "@frost/shared/Components/ui/slider";
import { Switch } from "@frost/shared/Components/ui/switch";
import { cn } from "@frost/shared/lib/utils";
import { cancelExport, exportStatus, openBlend, readImage, revealShot, startExport, type JobStatus } from "@/api/cinema";
import { useT, type TKey } from "@/i18n";

type Look = "matched" | "cinematic" | "realism";
type Shot = "view" | "photo" | "close" | "tight";
type Side = "sun" | "shade";

/** The stages the job announces, in order. */
const STEPS = ["prepare", "scene", "animate", "drive", "contact", "outfit", "pose", "fx", "shots", "save", "render"] as const;
type Step = (typeof STEPS)[number];

/** A code from the backend (`gameRunning`, `bikeNotInstalled: …`) in the rider's words. */
export function errorText(t: (k: TKey, v?: Record<string, string | number>) => string, raw: string): string {
  const [code, ...rest] = raw.split(":");
  const key = `error.${code.trim()}` as TKey;
  const text = t(key);
  if (text !== key) return text;
  return rest.length ? raw : t("error.generic", { detail: raw });
}

const mmss = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

/** The last line of the log that says what went wrong. */
function lastError(log: string[]): string | null {
  for (let i = log.length - 1; i >= 0; i--) {
    if (/error|exception|traceback/i.test(log[i]) && !/deprecat/i.test(log[i])) return log[i];
  }
  return null;
}

/**
 * The frame under the playhead, exported from the preview's view: one button, an optional picture,
 * then a progress bar until Blender is done, and the shot.
 */
export default function ExportPanel({
  path,
  mark,
  view,
  onBusy,
}: {
  path: string;
  mark: number;
  /** Reads the preview's camera at the moment Export is pressed. */
  view: () => ViewerCamera | null;
  onBusy?: (busy: boolean) => void;
}) {
  const t = useT();
  const [wantStill, setWantStill] = useState(false);
  const [look, setLook] = useState<Look>("matched");
  const [shot, setShot] = useState<Shot>("view");
  const [side, setSide] = useState<Side>("sun");
  // Percent: mud on the bike + dust on the gear, background blur, motion blur.
  const [mud, setMud] = useState(100);
  const [blur, setBlur] = useState(80);
  const [motion, setMotion] = useState(50);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [still, setStill] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [launching, setLaunching] = useState(false);
  const timer = useRef<number | null>(null);
  const live = useRef(true);

  const running = starting || job?.state === "preparing" || job?.state === "blender";
  useEffect(() => onBusy?.(running), [running, onBusy]);

  const poll = () => {
    exportStatus()
      .then((s) => {
        if (!live.current) return;
        setJob(s);
        if (s.state === "preparing" || s.state === "blender") {
          timer.current = window.setTimeout(poll, 700);
        } else if (s.state === "done" && s.still) {
          readImage(s.still).then((d) => live.current && setStill(d)).catch(() => {});
        }
      })
      .catch(() => {})
      .finally(() => live.current && setStarting(false));
  };
  // A job left running (the page was changed) is picked up again.
  useEffect(() => {
    live.current = true;
    poll();
    return () => {
      live.current = false;
      if (timer.current != null) window.clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const go = async () => {
    if (running) return;
    setStill(null);
    setStarting(true);
    const cam = view();
    const w = 1920;
    const h = cam ? Math.round(w / Math.max(0.5, Math.min(3, cam.aspect)) / 2) * 2 : 1080;
    try {
      await startExport({
        path,
        start: mark,
        end: mark,
        mark,
        render: wantStill ? "still" : "none",
        res: [w, h],
        samples: look === "matched" ? 128 : 384,
        camera: "auto",
        rider: true,
        fx: true,
        view: cam,
        look,
        shot,
        side,
        mud: mud / 100,
        blur: blur / 100,
        motion: motion / 100,
      });
      poll();
    } catch (e) {
      setStarting(false);
      toast.error(errorText(t, String(e)));
    }
  };

  // Launching Blender or Explorer: one at a time, and the button rests a few seconds after.
  const launch = (job: () => Promise<void>) => {
    if (launching) return;
    setLaunching(true);
    job()
      .catch((e) => toast.error(errorText(t, String(e))))
      .finally(() => window.setTimeout(() => live.current && setLaunching(false), 3000));
  };

  const steps: Step[] = STEPS.filter((s) => s !== "render" || job?.step === "render" || wantStill);
  const cur = job ? steps.indexOf(job.step as Step) : -1;
  const done = job?.state === "done";
  const pct = done ? 100 : cur < 0 ? 2 : Math.round((cur / steps.length) * 100);

  return (
    <div className="flex flex-col gap-4 rounded-xl bg-card p-[18px]">
      <div className="flex items-center gap-2">
        <Clapperboard className="size-4 text-primary" />
        <span className="flex-1 text-[14px] font-bold">{t("export.title")}</span>
        <span className="font-mono text-[11.5px] text-muted-foreground">{t("export.frameAt", { t: mark.toFixed(2) })}</span>
      </div>
      <div className="text-[12px] leading-relaxed text-muted-foreground">{t("export.hint")}</div>
      <label className={cn("flex items-center gap-2 text-[12.5px] text-foreground/85", running && "opacity-50")} title={t("export.stillTip")}>
        <Switch checked={wantStill} onCheckedChange={setWantStill} disabled={running} />
        {t("export.makeStill")}
      </label>
      {wantStill && (
        <div className={cn("space-y-2.5 text-[12px]", running && "pointer-events-none opacity-50")}>
          <div className="space-y-1">
            <div className="text-muted-foreground">{t("export.look")}</div>
            <Segmented<Look>
              size="sm"
              className="flex-wrap"
              value={look}
              onChange={setLook}
              options={[
                { value: "matched", label: t("export.lookMatched") },
                { value: "cinematic", label: t("export.lookCinematic") },
                { value: "realism", label: t("export.lookRealism") },
              ]}
            />
          </div>
          <div className="space-y-1">
            <div className="text-muted-foreground">{t("export.shot")}</div>
            <Segmented<Shot>
              size="sm"
              className="flex-wrap"
              value={shot}
              onChange={setShot}
              options={[
                { value: "view", label: t("export.shotView") },
                { value: "photo", label: t("export.shotPhoto") },
                { value: "close", label: t("export.shotClose") },
                { value: "tight", label: t("export.shotTight") },
              ]}
            />
          </div>
          {shot !== "view" && (
            <Segmented<Side>
              size="sm"
              value={side}
              onChange={setSide}
              options={[
                { value: "sun", label: t("export.sideSun") },
                { value: "shade", label: t("export.sideShade") },
              ]}
            />
          )}
          {look === "realism" && <Percent label={t("export.mud")} value={mud} onChange={setMud} />}
          {look !== "matched" && <Percent label={t("export.blur")} value={blur} onChange={setBlur} />}
          {look !== "matched" && <Percent label={t("export.motion")} value={motion} onChange={setMotion} />}
        </div>
      )}

      {running ? (
        <div className="space-y-2">
          <div className="text-[12.5px]">
            {t("export.stepOf", {
              n: Math.max(1, cur + 1),
              of: steps.length,
              name: t(`step.${(cur >= 0 ? steps[cur] : "prepare") as Step}` as TKey),
              time: mmss(job?.elapsed ?? 0),
            })}
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-black/30">
            <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${pct}%` }} />
          </div>
          <ul className="space-y-0.5">
            {steps.map((s, i) => (
              <li key={s} className={cn("flex items-center gap-2 text-[11.5px]", i < cur ? "text-foreground" : i === cur ? "text-primary" : "text-faint")}>
                {i < cur ? <Check className="size-3" /> : i === cur ? <Loader2 className="size-3 animate-spin" /> : <span className="size-3" />}
                {t(`step.${s}` as TKey)}
              </li>
            ))}
          </ul>
          <Button variant="outline" onClick={() => void cancelExport().then(poll)} title={t("export.cancelTip")}>
            <Ban className="size-3.5" />
            {t("export.cancel")}
          </Button>
        </div>
      ) : (
        <Button size="lg" onClick={() => void go()} title={t("export.goTip")}>
          {t("export.go")}
        </Button>
      )}

      {!running && job?.state === "failed" && (
        <div className="space-y-1">
          <div className="text-[12.5px] text-[#ff6961]">{errorText(t, job.error ?? "blenderFailed")}</div>
          {lastError(job.log) && <div className="break-all font-mono text-[10.5px] text-muted-foreground">{lastError(job.log)}</div>}
        </div>
      )}
      {!running && job?.state === "cancelled" && <div className="text-[12.5px] text-muted-foreground">{t("export.cancelled")}</div>}
      {!running && done && (
        <div className="space-y-2 border-t border-border pt-3">
          <div className="flex items-center gap-2 text-[12.5px] font-semibold">
            <Check className="size-4 text-[#30d158]" />
            {t("export.done", { s: Math.round(job!.elapsed) })}
          </div>
          {job!.blend && (
            <div className="select-text break-all rounded bg-black/30 px-2 py-1.5 font-mono text-[11px] text-foreground/85">{job!.blend}</div>
          )}
          {still && <img src={still} alt={t("export.stillAlt")} className="w-full rounded-md border border-border" />}
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" disabled={!job!.blend || launching} onClick={() => launch(() => openBlend(job!.blend!))} title={t("export.openTip")}>
              {t("export.openBlender")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={!job!.blend || launching}
              onClick={() => launch(() => revealShot(job!.blend!))}
              title={t("export.folderTip")}
            >
              <FolderOpen className="size-3.5" />
              {t("export.showFolder")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

/** A 0-100 % slider with its label and value on one line. */
function Percent({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  return (
    <div className="space-y-1">
      <div className="flex justify-between text-muted-foreground">
        <span>{label}</span>
        <span className="font-mono">{value}%</span>
      </div>
      <Slider min={0} max={100} step={5} value={[value]} onValueChange={(v) => onChange(v[0] ?? 0)} />
    </div>
  );
}
