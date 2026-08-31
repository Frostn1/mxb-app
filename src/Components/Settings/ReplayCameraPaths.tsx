import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  Download,
  RotateCcw,
  Ruler,
  Save,
  Trash2,
  TriangleAlert,
  Upload,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  rcamDelete,
  rcamExport,
  rcamImport,
  rcamPaths,
  rcamRead,
  rcamRetime,
  rcamStatus,
  rcamWrite,
  type RcamAnchor,
  type RcamCurve,
  type RcamEase,
  type RcamKey,
  type RcamPath,
  type RcamRig,
  type RcamStatus,
  type RcamSummary,
} from "../../api/mods";
import { useI18n } from "../../i18n/context";
import type { TFunc } from "../../i18n/core";
import { Button } from "@/Components/ui/button";
import { Segmented } from "@/Components/ui/segmented";
import { Switch } from "@/Components/ui/switch";
import { cn } from "@/lib/utils";

/** The replay's own sample rate. Every key time is a multiple of it, in game and here. */
const SAMPLE_MS = 30;

/** mm:ss.t — the same shape FrostMod's panel prints, so a time reads the same in both. */
function clockText(ms: number): string {
  const v = Math.max(0, ms);
  return `${Math.floor(v / 60000)}:${String(Math.floor(v / 1000) % 60).padStart(2, "0")}.${Math.floor((v % 1000) / 100)}`;
}

function snap(ms: number): number {
  return Math.round(ms / SAMPLE_MS) * SAMPLE_MS;
}

/** Each ease gets its own colour, and it is the same one FrostMod draws in the world. */
const EASE_TONE: Record<RcamEase, string> = {
  smooth: "bg-sky-400",
  hold: "bg-amber-300",
  cut: "bg-orange-400",
};

function easeLabel(t: TFunc, e: RcamEase): string {
  return t(e === "hold" ? "settings.rcamEaseHold" : e === "cut" ? "settings.rcamEaseCut" : "settings.rcamEaseSmooth");
}

/** The keys laid out on the path's own span, draggable. Seeing where the keys actually
 *  fall is the thing the in-game text panel cannot show you at all. */
function Timeline({
  keys,
  selected,
  onSelect,
  onMove,
}: {
  keys: RcamKey[];
  selected: number | null;
  onSelect: (i: number) => void;
  onMove: (i: number, t: number) => void;
}) {
  const track = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState<number | null>(null);

  const first = keys.length ? keys[0].t : 0;
  const last = keys.length ? keys[keys.length - 1].t : 0;
  const span = Math.max(1, last - first);
  const at = useCallback((t: number) => ((t - first) / span) * 100, [first, span]);

  // A key can be dragged anywhere between its neighbours but never onto them: crossing one
  // would reorder the path under the person editing it.
  const move = useCallback(
    (i: number, clientX: number) => {
      const box = track.current?.getBoundingClientRect();
      if (!box || box.width <= 0) return;
      const frac = Math.min(1, Math.max(0, (clientX - box.left) / box.width));
      const lo = i > 0 ? keys[i - 1].t + SAMPLE_MS : Number.NEGATIVE_INFINITY;
      const hi = i + 1 < keys.length ? keys[i + 1].t - SAMPLE_MS : Number.POSITIVE_INFINITY;
      onMove(i, Math.min(hi, Math.max(lo, snap(first + frac * span))));
    },
    [keys, first, span, onMove],
  );

  useEffect(() => {
    if (dragging === null) return;
    const onMoveEvent = (e: PointerEvent) => move(dragging, e.clientX);
    const onUp = () => setDragging(null);
    window.addEventListener("pointermove", onMoveEvent);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMoveEvent);
      window.removeEventListener("pointerup", onUp);
    };
  }, [dragging, move]);

  return (
    <div
      ref={track}
      className="relative mt-2 h-9 select-none rounded-md border border-input bg-card"
    >
      <div className="absolute inset-x-2 top-1/2 h-px -translate-y-1/2 bg-border" />
      {keys.map((k, i) => (
        <button
          key={`${k.t}-${i}`}
          type="button"
          title={`${clockText(k.t)} · ${k.ease}${k.target >= 0 ? ` · #${k.target}` : ""}`}
          onPointerDown={(e) => {
            e.preventDefault();
            onSelect(i);
            setDragging(i);
          }}
          className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2 p-1.5"
          style={{ left: `calc(8px + ${at(k.t)}% - ${(at(k.t) / 100) * 16}px)` }}
        >
          <span
            className={cn(
              "block size-2.5 rotate-45 rounded-[2px] transition-transform",
              EASE_TONE[k.ease],
              selected === i && "scale-150 ring-2 ring-foreground/40",
            )}
          />
          {k.target >= 0 && (
            <span className="absolute left-1/2 top-1/2 size-4 -translate-x-1/2 -translate-y-1/2 rounded-full border border-emerald-400/70" />
          )}
        </button>
      ))}
    </div>
  );
}

function SlotRow({
  summary,
  expanded,
  onToggle,
  onChanged,
}: {
  summary: RcamSummary;
  expanded: boolean;
  onToggle: () => void;
  onChanged: () => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<RcamPath | null>(null);
  const [clean, setClean] = useState<string>("");     // the draft as it was loaded
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const dirty = useMemo(
    () => (draft ? JSON.stringify(draft) !== clean : false),
    [draft, clean],
  );

  const load = useCallback(async () => {
    if (!summary.exists) return;
    try {
      const p = await rcamRead(summary.slot);
      setDraft(p);
      setClean(JSON.stringify(p));
      setSelected(null);
    } catch (e) {
      toast.error(t("settings.rcamReadFailed"), { description: String(e) });
    }
  }, [summary.exists, summary.slot, t]);

  useEffect(() => {
    if (expanded && !draft) void load();
  }, [expanded, draft, load]);

  const patch = useCallback((next: Partial<RcamPath>) => {
    setDraft((d) => (d ? { ...d, ...next } : d));
  }, []);

  const patchKey = useCallback((i: number, next: Partial<RcamKey>) => {
    setDraft((d) => {
      if (!d) return d;
      const keys = d.keys.map((k, j) => (j === i ? { ...k, ...next } : k));
      keys.sort((a, b) => a.t - b.t);
      return { ...d, keys };
    });
  }, []);

  const save = useCallback(async () => {
    if (!draft) return;
    setBusy(true);
    try {
      const file = await rcamWrite(summary.slot, draft);
      setClean(JSON.stringify(draft));
      toast.success(t("settings.rcamSaved"), { description: file });
      onChanged();
    } catch (e) {
      toast.error(t("settings.rcamWriteFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }, [draft, summary.slot, t, onChanged]);

  const retime = useCallback(async () => {
    if (!draft) return;
    try {
      setDraft(await rcamRetime(draft));
      toast.success(t("settings.rcamRetimeDone"));
    } catch (e) {
      toast.error(t("settings.rcamRetimeFailed"), { description: String(e) });
    }
  }, [draft, t]);

  const remove = useCallback(async () => {
    setBusy(true);
    try {
      await rcamDelete(summary.slot);
      setDraft(null);
      toast.success(t("settings.rcamDeleted", { n: summary.slot }));
      onChanged();
    } catch (e) {
      toast.error(t("settings.rcamDeleteFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }, [summary.slot, t, onChanged]);

  const doImport = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "FrostMod camera path", extensions: ["fcam"] }],
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      const p = await rcamImport(summary.slot, picked);
      setDraft(p);
      setClean(JSON.stringify(p));
      toast.success(t("settings.rcamImported", { n: summary.slot }));
      onChanged();
    } catch (e) {
      toast.error(t("settings.rcamImportFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }, [summary.slot, t, onChanged]);

  const doExport = useCallback(async () => {
    const dst = await saveDialog({
      defaultPath: `slot${summary.slot}.fcam`,
      filters: [{ name: "FrostMod camera path", extensions: ["fcam"] }],
    });
    if (!dst) return;
    try {
      await rcamExport(summary.slot, dst);
      toast.success(t("settings.rcamExported"), { description: dst });
    } catch (e) {
      toast.error(t("settings.rcamExportFailed"), { description: String(e) });
    }
  }, [summary.slot, t]);

  const key = selected !== null && draft ? draft.keys[selected] : null;

  return (
    <div className="rounded-md border border-input bg-card/40">
      <div className="flex items-center gap-2 px-2.5 py-2">
        <button
          type="button"
          onClick={onToggle}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
        >
          <ChevronDown
            className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform",
              expanded && "rotate-180")}
          />
          <span className="text-[12.5px] text-foreground/85">
            {t("settings.rcamSlot", { n: summary.slot })}
          </span>
          {summary.error ? (
            <span className="flex items-center gap-1 text-[11px] text-amber-500">
              <TriangleAlert className="size-3" /> {summary.error}
            </span>
          ) : summary.exists ? (
            <span className="truncate text-[11px] text-muted-foreground">
              {t("settings.rcamKeyCount", { count: summary.keys })} ·{" "}
              {t("settings.rcamShotCount", { count: summary.shots })} ·{" "}
              {clockText(summary.firstMs)}–{clockText(summary.lastMs)}
              {summary.targets.length > 0 &&
                ` · ${summary.targets.map((n) => `#${n}`).join(" ")}`}
            </span>
          ) : (
            <span className="text-[11px] text-muted-foreground">
              {t("settings.rcamSlotEmpty")}
            </span>
          )}
        </button>
        {!summary.exists && (
          <Button variant="ghost" size="sm" onClick={() => void doImport()} disabled={busy}>
            <Upload className="size-3.5" /> {t("settings.rcamImport")}
          </Button>
        )}
      </div>

      {expanded && summary.exists && draft && (
        <div className="border-t border-input px-2.5 py-2.5">
          <Timeline
            keys={draft.keys}
            selected={selected}
            onSelect={setSelected}
            onMove={(i, at) => patchKey(i, { t: at })}
          />
          <p className="mt-1.5 text-[11px] text-muted-foreground">
            {t("settings.rcamTimelineHint")}
          </p>

          {key ? (
            <div className="mt-2.5 flex flex-wrap items-center gap-3 rounded-md border border-input bg-background px-2.5 py-2">
              <span className="text-[12px] text-foreground/85">
                {t("settings.rcamKeyAt", { time: clockText(key.t) })}
              </span>
              <Segmented<RcamEase>
                size="sm"
                value={key.ease}
                onChange={(ease) => patchKey(selected!, { ease })}
                options={(["smooth", "hold", "cut"] as RcamEase[]).map((e) => ({
                  value: e,
                  label: easeLabel(t, e),
                }))}
              />
              <label className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
                {t("settings.rcamAim")}
                <input
                  type="number"
                  value={key.target < 0 ? "" : key.target}
                  placeholder={t("settings.rcamAimNone")}
                  onChange={(e) =>
                    patchKey(selected!, {
                      target: e.target.value === "" ? -1 : Number(e.target.value),
                    })
                  }
                  className="h-6 w-16 rounded border border-input bg-card px-1.5 text-[11px] text-foreground"
                />
              </label>
            </div>
          ) : (
            <p className="mt-2.5 text-[11px] text-muted-foreground">
              {t("settings.rcamNoKeySelected")}
            </p>
          )}

          <div className="mt-3 grid gap-2.5 sm:grid-cols-2">
            <div>
              <span className="text-[11px] text-muted-foreground">{t("settings.rcamCurve")}</span>
              <Segmented<RcamCurve>
                size="sm"
                className="mt-1"
                value={draft.curve}
                onChange={(curve) => patch({ curve })}
                options={[
                  { value: "centripetal", label: t("settings.rcamCurveCentripetal") },
                  { value: "uniform", label: t("settings.rcamCurveUniform") },
                ]}
              />
            </div>
            <div>
              <span className="text-[11px] text-muted-foreground">{t("settings.rcamAxis")}</span>
              <Segmented<RcamAnchor>
                size="sm"
                className="mt-1"
                value={draft.anchor}
                onChange={(anchor) => patch({ anchor })}
                options={[
                  { value: "clock", label: t("settings.rcamAxisClock") },
                  { value: "track", label: t("settings.rcamAxisTrack") },
                ]}
              />
            </div>
            <div className="sm:col-span-2">
              <span className="text-[11px] text-muted-foreground">{t("settings.rcamRig")}</span>
              <div className="mt-1 flex flex-wrap items-center gap-3">
                <Segmented<RcamRig>
                  size="sm"
                  value={draft.rig}
                  onChange={(rig) => patch({ rig })}
                  options={[
                    { value: "locked", label: t("settings.rcamRigLocked") },
                    { value: "handheld", label: t("settings.rcamRigHandheld") },
                    { value: "drone", label: t("settings.rcamRigDrone") },
                    { value: "crane", label: t("settings.rcamRigCrane") },
                  ]}
                />
                {draft.rig !== "locked" && (
                  <label className="flex items-center gap-2 text-[11px] text-muted-foreground">
                    {t("settings.rcamRigAmount")}
                    <input
                      type="range"
                      min={0}
                      max={200}
                      step={5}
                      value={Math.round(draft.rigAmount * 100)}
                      onChange={(e) => patch({ rigAmount: Number(e.target.value) / 100 })}
                      className="w-28"
                    />
                    <span className="tabular-nums">{Math.round(draft.rigAmount * 100)}%</span>
                  </label>
                )}
              </div>
            </div>
            <label className="flex items-center justify-between gap-3 sm:col-span-2">
              <span className="flex flex-col">
                <span className="text-[12px] text-foreground/85">{t("settings.rcamAutoFov")}</span>
                <span className="text-[11px] text-muted-foreground">
                  {t("settings.rcamAutoFovHint")}
                </span>
              </span>
              <Switch
                checked={draft.autoFov}
                onCheckedChange={(autoFov) => patch({ autoFov })}
              />
            </label>
          </div>

          {draft.anchor === "track" && (
            <p className="mt-2 text-[11px] text-muted-foreground">{t("settings.rcamAxisHint")}</p>
          )}

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button size="sm" onClick={() => void save()} disabled={!dirty || busy}>
              <Save className="size-3.5" /> {t("settings.rcamSave")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void load()}
              disabled={!dirty || busy}
            >
              <RotateCcw className="size-3.5" /> {t("settings.rcamRevert")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void retime()} disabled={busy}>
              <Ruler className="size-3.5" /> {t("settings.rcamRetime")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void doExport()} disabled={busy}>
              <Download className="size-3.5" /> {t("settings.rcamExport")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void doImport()} disabled={busy}>
              <Upload className="size-3.5" /> {t("settings.rcamImport")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void remove()}
              disabled={busy}
              className="text-destructive hover:text-destructive"
            >
              <Trash2 className="size-3.5" /> {t("settings.rcamDelete")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

/** The nine slots FrostMod saves camera paths to.
 *
 *  In game the editor is a text panel over a running replay: right for setting a key while
 *  you scrub, no use for looking at what you saved last week. This is the other half - what
 *  is in each slot, where the keys fall, how the path flies, and a way to pass one on.
 */
export function ReplayCameraPaths() {
  const { t } = useI18n();
  const [slots, setSlots] = useState<RcamSummary[] | null>(null);
  const [status, setStatus] = useState<RcamStatus | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);

  const refresh = useCallback(() => {
    void rcamPaths()
      .then(setSlots)
      .catch(() => setSlots([]));
    void rcamStatus()
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  useEffect(refresh, [refresh]);

  if (!slots) return null;
  const any = slots.some((s) => s.exists);

  return (
    <div className="rounded-lg border border-input bg-background px-3 py-2.5">
      <div className="flex items-start justify-between gap-3">
        <div className="flex flex-col">
          <span className="text-[12.5px] text-foreground/85">{t("settings.rcamPaths")}</span>
          <span className="text-[11px] leading-relaxed text-muted-foreground">
            {t("settings.rcamPathsDesc")}
          </span>
        </div>
        <Button variant="outline" size="sm" onClick={refresh}>
          <RotateCcw className="size-3.5" /> {t("settings.rcamRefresh")}
        </Button>
      </div>

      {/* The camera globals are resolved by signature and MX Bikes moves them between
          builds, so "the editor opens and does nothing" is a real state. Its reason lived
          in a log file next to a DLL until now. */}
      {status?.known && !status.ready && status.reason && (
        <p className="mt-2.5 flex items-start gap-1.5 text-[11px] text-amber-500">
          <TriangleAlert className="mt-px size-3 shrink-0" />
          {t("settings.rcamUnavailable", { reason: status.reason })}
        </p>
      )}
      {status?.ready && !status.calibrated && (
        <p className="mt-2.5 text-[11px] text-muted-foreground">
          {t("settings.rcamNotCalibrated")}
        </p>
      )}

      {!any && (
        <p className="mt-2.5 text-[11px] text-muted-foreground">{t("settings.rcamPathsEmpty")}</p>
      )}

      <div className="mt-2.5 grid gap-1.5">
        {slots
          .filter((s) => s.exists || expanded === s.slot || !any)
          .map((s) => (
            <SlotRow
              key={s.slot}
              summary={s}
              expanded={expanded === s.slot}
              onToggle={() => setExpanded(expanded === s.slot ? null : s.slot)}
              onChanged={refresh}
            />
          ))}
      </div>
    </div>
  );
}
