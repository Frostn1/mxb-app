import { useEffect, useMemo, useRef, useState } from "react";
import { FileBox, FolderOpen, Loader2, RotateCcw } from "lucide-react";
import { Button } from "../ui/button";
import { Slider } from "../ui/controls";
import { ErrorBoundary } from "../ErrorBoundary";
import { ModelViewer, type Shaders } from "../Viewer/ModelViewer";
import { loadModelFile, revealInExplorer, type LooseModel } from "../../api/mods";
import { nameOf, STOCK_SHD, writeShd, type Outcome, type ShdFile, type ShdValues } from "../../api/convert";
import { useT } from "../../i18n/context";

/** How long a slider rests before its `.shd` is written: dragging is many values, one file. */
const SAVE_AFTER = 250;

const head = "text-[11px] font-semibold uppercase tracking-[0.09em] text-faint";

/**
 * What a converted model came to: every file written, the model in 3D, and its materials'
 * `.shd` values on sliders. The view follows the sliders as they move and each change is
 * written to the `.shd` beside the `.edf`.
 *
 * Mount it per conversion (`key`): the values start from what the conversion left on disk.
 */
export default function ConvertResult({ output, outcome }: { output: string; outcome: Outcome }) {
  const t = useT();
  const [model, setModel] = useState<LooseModel | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [values, setValues] = useState<Record<string, ShdValues>>(() => Object.fromEntries(outcome.shaders.map((f) => [f.path, f.values])));
  const [saveError, setSaveError] = useState<string | null>(null);

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
    <div className="mt-5 grid gap-6">
      <section>
        <div className="flex items-center">
          <h3 className={head}>{t("convert.written")}</h3>
          {outcome.files.length > 0 && (
            <Button size="sm" variant="ghost" className="ml-auto" onClick={() => void revealInExplorer(outcome.files[0])}>
              <FolderOpen className="size-3.5" /> {t("convert.openFolder")}
            </Button>
          )}
        </div>
        <p className="mb-2 text-[11.5px] text-muted-foreground">{t("convert.writtenHint")}</p>
        <ul className="border border-border bg-card text-[12px]">
          {outcome.files.map((f) => (
            <li key={f} className="flex items-center gap-2 border-b border-border/60 px-3 py-1.5 last:border-b-0" title={f}>
              <FileBox className="size-3.5 flex-none text-muted-foreground" />
              <span className="select-text truncate font-mono">{nameOf(f)}</span>
            </li>
          ))}
        </ul>
      </section>

      <section>
        <h3 className={head}>{t("convert.view")}</h3>
        <p className="mb-2 text-[11.5px] text-muted-foreground">{t("convert.viewHint")}</p>
        <div className="relative h-[340px] overflow-hidden rounded-lg border border-border bg-card">
          {failed ? (
            <p className="flex h-full items-center justify-center px-6 text-center text-[12px] text-muted-foreground">{t("convert.viewFailed", { why: failed })}</p>
          ) : !model ? (
            <div className="flex h-full items-center justify-center text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
            </div>
          ) : (
            <ErrorBoundary compact label="convert-view">
              <ModelViewer mode="bike" nodes={model.nodes} textures={model.textures} shaders={shaders} hideHints className="absolute inset-0" />
            </ErrorBoundary>
          )}
        </div>
      </section>

      <section>
        <h3 className={head}>{t("convert.shaders")}</h3>
        <p className="mb-2 text-[11.5px] text-muted-foreground">{t("convert.shadersHint")}</p>
        {outcome.shaders.length === 0 ? (
          <p className="text-[12px] text-muted-foreground">{t("convert.noShaders")}</p>
        ) : (
          <div className="grid gap-3">
            {outcome.shaders.map((f) => (
              <Material key={f.path} file={f} values={values[f.path] ?? f.values} onChange={(v) => change(f, v)} />
            ))}
          </div>
        )}
        {saveError && <p className="mt-2 select-text text-[11.5px] text-destructive">{saveError}</p>}
      </section>
    </div>
  );
}

function Material({ file, values, onChange }: { file: ShdFile; values: ShdValues; onChange: (v: ShdValues) => void }) {
  const t = useT();
  const set = (k: keyof ShdValues) => (n: number) => onChange({ ...values, [k]: n });
  const stock = (Object.keys(STOCK_SHD) as (keyof ShdValues)[]).every((k) => values[k] === STOCK_SHD[k]);
  return (
    <div className="rounded-lg border border-border bg-card/40 px-3.5 py-3">
      <div className="mb-2 flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-[12px]" title={file.path}>
          {file.name}.shd
        </span>
        {!file.written && <span className="flex-none rounded-full bg-muted px-2 py-0.5 text-[10.5px] text-muted-foreground">{t("convert.shdKept")}</span>}
        <Button size="sm" variant="ghost" className="h-6 px-1.5" disabled={stock} title={t("convert.resetShd")} onClick={() => onChange({ ...STOCK_SHD })}>
          <RotateCcw className="size-3.5" />
        </Button>
      </div>
      <div className="grid gap-1.5">
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
