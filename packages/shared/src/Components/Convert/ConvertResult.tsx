import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronRight, Eye, FileBox, FolderOpen, Loader2, RotateCcw } from "lucide-react";
import { Button } from "../ui/button";
import { Segmented } from "../ui/segmented";
import { Slider } from "../ui/controls";
import { cn } from "../../lib/utils";
import { ErrorBoundary } from "../ErrorBoundary";
import { ModelViewer, type Shaders } from "../Viewer/ModelViewer";
import { contextBike, contextBikes, contextRiderBody, loadModelFile, revealInExplorer, type LooseModel } from "../../api/mods";
import type { BikeModel, RiderPart } from "../../types";
import { nameOf, STOCK_SHD, writeShd, type Outcome, type ShdFile, type ShdValues } from "../../api/convert";
import { useT } from "../../i18n/context";

/** How long a slider rests before its `.shd` is written: dragging is many values, one file. */
const SAVE_AFTER = 250;

const head = "text-[11px] font-semibold tracking-[0.09em] text-faint";

/** Every file a conversion wrote, with the one Open folder. */
export function WrittenFiles({ outcome }: { outcome: Outcome }) {
  const t = useT();
  return (
    <section className="mt-5">
      <div className="flex items-center">
        <h3 className={head}>{t("convert.written")}</h3>
        {outcome.files.length > 0 && (
          <Button size="sm" variant="ghost" className="ml-auto" onClick={() => void revealInExplorer(outcome.files[0])}>
            <FolderOpen className="size-3.5" /> {t("convert.openFolder")}
          </Button>
        )}
      </div>
      <ul className="border border-border bg-card text-[12px]">
        {outcome.files.map((f) => (
          <li key={f} className="flex items-center gap-2 border-b border-border/60 px-3 py-1.5 last:border-b-0" title={f}>
            <FileBox className="size-3.5 flex-none text-muted-foreground" />
            <span className="select-text truncate font-mono">{nameOf(f)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * The Preview tab: the last conversion in 3D with its materials' `.shd` values on sliders, the
 * view large beside a list that scrolls on its own (stacked on a narrow window, the view on
 * top). The view follows the sliders as they move and each change is written to the `.shd`
 * beside the `.edf`; picking a material lights it up in the view.
 *
 * `watching` converts again whenever a source file is saved (see Convert).
 */
export function ConvertPreview({
  last,
  running,
  watching,
  canWatch,
  onWatch,
  onConvert,
}: {
  last: { key: string; output: string; outcome: Outcome } | null;
  running: boolean;
  watching: boolean;
  canWatch: boolean;
  onWatch: (on: boolean) => void;
  onConvert: () => void;
}) {
  const t = useT();
  const [ctx, setCtx] = useState<Context>("model");
  const [gear, setGear] = useState<Gear>("protection");
  const [bikes, setBikes] = useState<string[]>([]);
  const [bike, setBike] = useState("");
  useEffect(() => {
    let alive = true;
    contextBikes()
      .then((b) => {
        if (!alive) return;
        setBikes(b);
        setBike((cur) => cur || (b.includes("450xf") ? "450xf" : (b[0] ?? "")));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  // A bike context needs the .hrc files the conversion wrote: they say which parts are the model's.
  const bikeOk = !!last && last.outcome.hrc.length > 0;
  const shown: Context = ctx === "bike" && !bikeOk ? "model" : ctx;
  if (!last)
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-[12.5px] text-muted-foreground">
        {running ? <Loader2 className="size-5 animate-spin" /> : <Eye className="size-6" />}
        {t("convert.previewEmpty")}
        {!running && (
          <Button size="sm" variant="outline" onClick={onConvert}>
            {t("convert.tabConvert")}
          </Button>
        )}
      </div>
    );
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-none items-center gap-3 px-6 pt-3 text-[12px]">
        <Segmented
          size="sm"
          value={shown}
          onChange={setCtx}
          options={[
            { value: "model", label: t("convert.ctxModel") },
            { value: "rider", label: t("convert.ctxRider") },
            ...(bikeOk ? [{ value: "bike" as const, label: t("convert.ctxBike") }] : []),
          ]}
        />
        {shown === "rider" && (
          <Segmented
            size="sm"
            value={gear}
            onChange={setGear}
            options={[
              { value: "protection", label: t("convert.gearProtection") },
              { value: "helmet", label: t("convert.gearHelmet") },
              { value: "boots", label: t("convert.gearBoots") },
            ]}
          />
        )}
        {shown === "bike" && bikes.length > 0 && (
          <select className="h-7 rounded-md border border-input bg-transparent px-2 text-[12px]" value={bike} onChange={(e) => setBike(e.target.value)}>
            {bikes.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        )}
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={watching} disabled={!canWatch} onChange={(e) => onWatch(e.target.checked)} />
          {t("convert.watch")}
        </label>
        <span className="ml-auto flex-none truncate font-mono text-[11.5px] text-muted-foreground" title={last.output}>
          {nameOf(last.output)}
        </span>
      </div>
      <PreviewBody key={last.key} output={last.output} outcome={last.outcome} ctx={shown} gear={gear} bike={bike} />
    </div>
  );
}

type Context = "model" | "rider" | "bike";
type Gear = "protection" | "helmet" | "boots";

/**
 * The model among the game's own files, read from the player's game folder and never copied
 * out: worn by the stock rider as a piece of gear, or laid over a stock bike so the parts it
 * wrote replace the stock ones. `null` while loading; `why` says what went wrong.
 */
function useAround(ctx: Context, output: string, outcome: Outcome, bike: string) {
  const [body, setBody] = useState<RiderPart | null>(null);
  const [onBike, setOnBike] = useState<BikeModel | null>(null);
  const [why, setWhy] = useState<string | null>(null);
  useEffect(() => {
    if (ctx === "model") return;
    let alive = true;
    setWhy(null);
    if (ctx === "rider") {
      contextRiderBody()
        .then((b) => alive && (b ? setBody(b) : setWhy("noGame")))
        .catch((e) => alive && setWhy(String(e).replace(/^Error:\s*/, "")));
    } else {
      setOnBike(null);
      if (!bike) setWhy("noGame");
      else
        contextBike(bike, output, outcome.hrc)
          .then((m) => alive && setOnBike(m))
          .catch((e) => alive && setWhy(String(e).replace(/^Error:\s*/, "")));
    }
    return () => {
      alive = false;
    };
  }, [ctx, output, outcome.hrc, bike]);
  return { body, onBike, why };
}

/** Mount per conversion (`key`): the values start from what the conversion left on disk. */
function PreviewBody({ output, outcome, ctx, gear, bike }: { output: string; outcome: Outcome; ctx: Context; gear: Gear; bike: string }) {
  const t = useT();
  const around = useAround(ctx, output, outcome, bike);
  const [model, setModel] = useState<LooseModel | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [values, setValues] = useState<Record<string, ShdValues>>(() => Object.fromEntries(outcome.shaders.map((f) => [f.path, f.values])));
  const [saveError, setSaveError] = useState<string | null>(null);
  /** The material picked in the list: lit in the view, its sliders open. */
  const [focus, setFocus] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setModel(null);
    setFailed(null);
    loadModelFile(output)
      .then((m) => alive && setModel(m))
      .catch((e) => alive && setFailed(String(e).replace(/^Error:\s*/, "")));
    return () => {
      alive = false;
    };
  }, [output]);

  // One pending write per file; leaving writes whatever is waiting, so a slider let go of and
  // then closed is not lost.
  const pending = useRef(new Map<string, { file: ShdFile; values: ShdValues; timer: number }>());
  const save = (file: ShdFile, v: ShdValues) =>
    writeShd(file.path, file.maps, v).then(
      () => setSaveError(null),
      (e) => setSaveError(String(e).replace(/^Error:\s*/, "")),
    );
  useEffect(() => {
    const waiting = pending.current;
    return () => {
      for (const p of waiting.values()) {
        window.clearTimeout(p.timer);
        void writeShd(p.file.path, p.file.maps, p.values).catch(() => {});
      }
      waiting.clear();
    };
  }, []);
  const change = (file: ShdFile, v: ShdValues) => {
    setValues((prev) => ({ ...prev, [file.path]: v }));
    const old = pending.current.get(file.path);
    if (old) window.clearTimeout(old.timer);
    const timer = window.setTimeout(() => {
      pending.current.delete(file.path);
      void save(file, v);
    }, SAVE_AFTER);
    pending.current.set(file.path, { file, values: v, timer });
  };

  const shaders = useMemo<Shaders>(() => Object.fromEntries(outcome.shaders.map((f) => [f.name.toLowerCase(), values[f.path] ?? f.values])), [outcome.shaders, values]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 px-6 pb-5 pt-3 lg:flex-row">
      <div className="relative h-[46%] min-h-[220px] flex-none overflow-hidden rounded-lg border border-border bg-card lg:h-auto lg:min-h-0 lg:flex-1">
        {failed || (ctx !== "model" && around.why) ? (
          <p className="flex h-full items-center justify-center px-6 text-center text-[12px] text-muted-foreground">
            {around.why === "noGame" && !failed ? t("convert.noGame") : t("convert.viewFailed", { why: failed ?? around.why ?? "" })}
          </p>
        ) : !model || (ctx === "rider" && !around.body) || (ctx === "bike" && !around.onBike) ? (
          <div className="flex h-full items-center justify-center text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
          </div>
        ) : (
          <ErrorBoundary compact label="convert-view">
            {ctx === "rider" && around.body ? (
              <ModelViewer
                mode="rider"
                riderParts={[around.body, { part: gear, nodes: model.nodes, textures: model.textures }]}
                hideHints
                className="absolute inset-0"
              />
            ) : ctx === "bike" && around.onBike ? (
              <ModelViewer
                mode="bike"
                nodes={around.onBike.nodes}
                rig={around.onBike.rig}
                textures={around.onBike.paints[0]?.textures ?? around.onBike.base}
                tyreTextures={around.onBike.tyreTextures}
                hideHints
                className="absolute inset-0"
              />
            ) : (
            <ModelViewer mode="bike" nodes={model.nodes} textures={model.textures} shaders={shaders} focusSheet={focus} hideHints className="absolute inset-0" />
            )}
          </ErrorBoundary>
        )}
      </div>

      <section className="min-h-0 flex-1 overflow-y-auto lg:w-[360px] lg:flex-none">
        <h3 className={head}>{t("convert.shaders")}</h3>
        {outcome.shaders.length === 0 ? (
          <p className="text-[12px] text-muted-foreground">{t("convert.noShaders")}</p>
        ) : (
          <div className="grid gap-2">
            {outcome.shaders.map((f) => (
              <Material
                key={f.path}
                file={f}
                values={values[f.path] ?? f.values}
                open={focus === f.name.toLowerCase()}
                onToggle={() => setFocus((cur) => (cur === f.name.toLowerCase() ? null : f.name.toLowerCase()))}
                onChange={(v) => change(f, v)}
              />
            ))}
          </div>
        )}
        {saveError && <p className="mt-2 select-text text-[11.5px] text-destructive">{saveError}</p>}
      </section>
    </div>
  );
}

/** One material: a line (name, kept, reset) that opens its sliders and lights it in the view. */
function Material({ file, values, open, onToggle, onChange }: { file: ShdFile; values: ShdValues; open: boolean; onToggle: () => void; onChange: (v: ShdValues) => void }) {
  const t = useT();
  const set = (k: keyof ShdValues) => (n: number) => onChange({ ...values, [k]: n });
  const stock = (Object.keys(STOCK_SHD) as (keyof ShdValues)[]).every((k) => values[k] === STOCK_SHD[k]);
  return (
    <div className={cn("rounded-lg border bg-card/40", open ? "border-primary/50" : "border-border")}>
      <div className="flex items-center gap-2 px-3 py-1.5">
        <button className="flex min-w-0 flex-1 cursor-default items-center gap-1.5 text-left" aria-expanded={open} onClick={onToggle}>
          <ChevronRight className={cn("size-3.5 flex-none text-muted-foreground transition-transform", open && "rotate-90")} />
          <span className="min-w-0 flex-1 truncate font-mono text-[12px]" title={file.path}>
            {file.name}.shd
          </span>
        </button>
        {!file.written && <span className="flex-none rounded-full bg-muted px-2 py-0.5 text-[10.5px] text-muted-foreground">{t("convert.shdKept")}</span>}
        <Button size="sm" variant="ghost" className="h-6 px-1.5" disabled={stock} title={t("convert.resetShd")} onClick={() => onChange({ ...STOCK_SHD })}>
          <RotateCcw className="size-3.5" />
        </Button>
      </div>
      {open && (
        <div className="grid gap-1.5 px-3.5 pb-3 pt-1">
          <Control label={t("convert.shininess")}>
            <Slider editable value={values.shininess} min={0} max={100} step={1} onChange={set("shininess")} format={(v) => String(v)} />
          </Control>
          <Control label={t("convert.reflectionMin")}>
            <Slider editable value={values.reflectionMin} min={0} max={1} step={0.01} onChange={set("reflectionMin")} format={(v) => v.toFixed(2)} />
          </Control>
          <Control label={t("convert.reflectionMax")}>
            <Slider editable value={values.reflectionMax} min={0} max={1} step={0.01} onChange={set("reflectionMax")} format={(v) => v.toFixed(2)} />
          </Control>
          <Control label={t("convert.reflectionExp")}>
            <Slider editable value={values.reflectionExp} min={0} max={4} step={0.1} onChange={set("reflectionExp")} format={(v) => v.toFixed(1)} />
          </Control>
        </div>
      )}
    </div>
  );
}

function Control({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex items-center gap-2">
      <span className="w-[120px] flex-none text-[11.5px] text-muted-foreground">{label}</span>
      <span className="flex min-w-0 flex-1 items-center gap-1.5">{children}</span>
    </label>
  );
}
