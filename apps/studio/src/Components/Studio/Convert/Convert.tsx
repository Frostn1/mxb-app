import { useContext, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { CheckCircle2, FileBox, FilePlus, FolderOpen, FolderPlus, Loader2, Play, TriangleAlert, X } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Progress } from "@frost/shared/Components/ui/progress";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { revealInExplorer } from "@frost/shared/api/mods";
import { PaneActive } from "../../Shell/ContextBar";
import {
  checkScript,
  convertFiles,
  dirOf,
  looksLikeBike,
  nameOf,
  onDone,
  onProgress,
  outputPaths,
  partNames,
  readText,
  scanPaths,
  type ConvertOptions,
  type Found,
  type Outcome,
} from "@/api/convert";
import { useT } from "@/i18n";

type Normals = "file" | "flat" | "recalc";

interface Settings {
  out: "beside" | "folder";
  folder: string;
  merge: boolean;
  mergeDistance: string;
  normals: Normals;
  angle: string;
  layout: "whole" | "parts";
  main: string;
  scale: string;
  fast: boolean;
  script: string;
  shd: boolean;
  overwriteShd: boolean;
}

// The browser converter's defaults, which are fbx2edf.exe's own dialog's.
const DEFAULTS: Settings = {
  out: "beside",
  folder: "",
  merge: false,
  mergeDistance: "0.0001",
  normals: "file",
  angle: "90",
  layout: "whole",
  main: "",
  scale: "1",
  fast: true,
  script: "",
  shd: true,
  overwriteShd: false,
};

const REMEMBER = "studio.convert";

function load(): Settings {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(REMEMBER) ?? "{}") };
  } catch {
    return DEFAULTS;
  }
}

type Status =
  | { state: "waiting" }
  | { state: "working"; done: number; total: number }
  | { state: "done"; outcome: Outcome };

interface Job {
  input: string;
  output: string;
  shadow: boolean;
  status: Status;
}

const mb = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1e3))} KB`);

/**
 * Convert: FBX files to the `.edf` MX Bikes loads, a batch at a time, on this machine. The
 * same converter and the same options as mxbsecure.com/convert, without the upload.
 */
export default function Convert() {
  const t = useT();
  const active = useContext(PaneActive);
  const [s, setS] = useState<Settings>(load);
  const [files, setFiles] = useState<Found[]>([]);
  const [off, setOff] = useState<Set<string>>(new Set());
  const [jobs, setJobs] = useState<Job[] | null>(null);
  const [running, setRunning] = useState(false);
  const [over, setOver] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const layoutTouched = useRef(false);

  useEffect(() => {
    try {
      localStorage.setItem(REMEMBER, JSON.stringify(s));
    } catch {
      // Blocked storage: the settings just aren't remembered.
    }
  }, [s]);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((p) => ({ ...p, [k]: v }));

  const models = useMemo(() => files.filter((f) => f.kind !== "params"), [files]);
  const chosen = useMemo(() => models.filter((f) => !off.has(f.path)), [models, off]);
  const params = files.find((f) => f.kind === "params") ?? null;
  const script = s.script.trim() ? checkScript(s.script) : null;

  const add = async (paths: string[]) => {
    if (running || paths.length === 0) return;
    const found = await scanPaths(paths);
    setJobs(null);
    setFiles((prev) => {
      const seen = new Set(prev.map((f) => f.path.toLowerCase()));
      return [...prev, ...found.filter((f) => !seen.has(f.path.toLowerCase()))];
    });
  };

  // Drops land on the whole pane while it is on screen.
  const addRef = useRef(add);
  addRef.current = add;
  useEffect(() => {
    if (!active) return;
    let un: (() => void) | undefined;
    let gone = false;
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          setOver(false);
          void addRef.current(e.payload.paths);
        } else setOver(e.payload.type === "over" || e.payload.type === "enter");
      })
      .then((f) => (gone ? f() : (un = f)));
    return () => {
      gone = true;
      un?.();
    };
  }, [active]);

  // A bike defaults to Parts, from the first model's own top-level names, until Layout is
  // picked by hand.
  const firstPath = (chosen.find((f) => f.kind === "model") ?? chosen[0])?.path;
  useEffect(() => {
    if (!firstPath || layoutTouched.current) return;
    let alive = true;
    partNames(firstPath)
      .then((names) => alive && !layoutTouched.current && set("layout", looksLikeBike(names) ? "parts" : "whole"))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [firstPath]);

  const pick = async (directory: boolean) => {
    const p = await openDialog({
      multiple: true,
      directory,
      filters: directory ? undefined : [{ name: "FBX", extensions: ["fbx", "ini"] }],
    });
    void add(Array.isArray(p) ? p : p ? [p] : []);
  };
  const pickFolder = async () => {
    const p = await openDialog({ directory: true, defaultPath: s.folder || undefined });
    if (typeof p === "string") set("folder", p);
  };
  const loadScript = async () => {
    const p = await openDialog({ filters: [{ name: "Script", extensions: ["ini", "txt", "params"] }] });
    if (typeof p === "string") set("script", await readText(p));
  };

  const options = async (): Promise<ConvertOptions | string> => {
    const num = (v: string, what: string) => (Number.isFinite(Number(v)) && v.trim() !== "" ? Number(v) : t("convert.notNumber", { what }));
    const scale = num(s.scale, t("convert.scale"));
    const merge = s.merge ? num(s.mergeDistance, t("convert.merge")) : 0;
    const angle = s.normals === "flat" ? 0 : num(s.angle, t("convert.recalc"));
    for (const v of [scale, merge, s.normals === "recalc" ? angle : 0]) if (typeof v === "string") return v;
    const text = s.script.trim() ? s.script : params ? await readText(params.path) : undefined;
    return {
      params: text,
      layout: s.layout,
      main: s.layout === "parts" && s.main.trim() ? s.main.trim() : undefined,
      scale: scale as number,
      mergeDistance: merge as number,
      ...(s.normals === "file" ? { useFileNormals: true } : { recalcNormals: angle as number }),
      lowMemory: !s.fast,
    };
  };

  const run = async () => {
    if (running || chosen.length === 0) return;
    setError(null);
    let o: ConvertOptions | string;
    try {
      o = await options();
    } catch (e) {
      o = String(e);
    }
    if (typeof o === "string") return setError(o);
    const list = chosen.map((f) => ({ path: f.path, shadow: f.kind === "shadow" }));
    const outs = outputPaths(list, s.out === "folder" && s.folder ? s.folder : null);
    const batch: Job[] = list.map((m, i) => ({ input: m.path, output: outs[i], shadow: m.shadow, status: { state: "waiting" } }));
    setJobs(batch);
    setRunning(true);
    const patch = (i: number, status: Status) => setJobs((js) => js && js.map((j, k) => (k === i ? { ...j, status } : j)));
    const unProgress = await onProgress((e) => patch(e.index, { state: "working", done: e.done, total: e.total }));
    const unDone = await onDone((e) => patch(e.index, { state: "done", outcome: e.outcome }));
    try {
      const results = await convertFiles(
        batch.map((j) => ({ input: j.input, output: j.output })),
        o,
        true,
        { make: s.shd, overwrite: s.shd && s.overwriteShd },
      );
      setJobs((js) => js && js.map((j, i) => ({ ...j, status: { state: "done", outcome: results[i] } })));
    } catch (e) {
      setError(String(e));
      setJobs(null);
    } finally {
      unProgress();
      unDone();
      setRunning(false);
    }
  };

  const doneCount = (jobs ?? []).filter((j) => j.status.state === "done" && j.status.outcome.report).length;
  const outFolder = s.out === "folder" && s.folder ? s.folder : jobs?.[0] ? dirOf(jobs[0].output) : null;

  return (
    <div className="flex h-full min-h-0">
      <section className="flex w-[340px] flex-none flex-col gap-4 overflow-y-auto border-r border-border px-6 py-4">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("convert.title")}</h2>
        <fieldset disabled={running} className="contents">
          <Field label={t("convert.output")}>
            <Segmented
              size="sm"
              value={s.out}
              onChange={(v) => set("out", v)}
              options={[
                { value: "beside", label: t("convert.beside") },
                { value: "folder", label: t("convert.folder") },
              ]}
            />
            {s.out === "folder" && (
              <div className="mt-2 flex items-center gap-1.5">
                <span
                  className={cn("min-w-0 flex-1 truncate rounded-lg border border-input px-2 py-1.5 font-mono text-[11.5px]", !s.folder && "text-faint")}
                  title={s.folder}
                >
                  {s.folder || t("convert.pickFolder")}
                </span>
                <Button size="sm" variant="outline" onClick={() => void pickFolder()}>
                  <FolderOpen className="size-3.5" />
                </Button>
              </div>
            )}
          </Field>

          <Field label={t("convert.mergeTitle")}>
            <div className="flex items-center gap-3">
              <label className="flex flex-none items-center gap-2 text-[12.5px]">
                <input type="checkbox" checked={s.merge} onChange={(e) => set("merge", e.target.checked)} />
                {t("convert.merge")}
              </label>
              <Input className="h-8" inputMode="decimal" disabled={!s.merge} value={s.mergeDistance} onChange={(e) => set("mergeDistance", e.target.value)} />
            </div>
          </Field>

          <Field label={t("convert.normals")}>
            <div className="grid gap-1.5 text-[12.5px]">
              {(["file", "flat", "recalc"] as const).map((n) => (
                <label key={n} className="flex h-8 items-center gap-2">
                  <input type="radio" name="convert-normals" checked={s.normals === n} onChange={() => set("normals", n)} />
                  <span className="flex-none">{t(n === "file" ? "convert.keep" : n === "flat" ? "convert.flat" : "convert.recalc")}</span>
                  {n === "recalc" && (
                    <Input
                      className="ml-auto h-8 w-20 text-right"
                      inputMode="decimal"
                      disabled={s.normals !== "recalc"}
                      value={s.angle}
                      onChange={(e) => set("angle", e.target.value)}
                    />
                  )}
                </label>
              ))}
            </div>
          </Field>

          <Field label={t("convert.layout")}>
            <Segmented
              size="sm"
              value={s.layout}
              onChange={(v) => {
                layoutTouched.current = true;
                set("layout", v);
              }}
              options={[
                { value: "whole", label: t("convert.whole") },
                { value: "parts", label: t("convert.parts") },
              ]}
            />
          </Field>

          <div className="grid grid-cols-2 gap-3">
            <Field label={t("convert.scale")}>
              <Input className="h-8" inputMode="decimal" value={s.scale} onChange={(e) => set("scale", e.target.value)} />
            </Field>
            {s.layout === "parts" && (
              <Field label={t("convert.main")}>
                <Input className="h-8" value={s.main} placeholder="chassis" onChange={(e) => set("main", e.target.value)} />
              </Field>
            )}
          </div>

          <label className="flex items-center gap-2 text-[12.5px]">
            <input type="checkbox" checked={s.fast} onChange={(e) => set("fast", e.target.checked)} />
            {t("convert.fast")}
          </label>

          <Field label={t("convert.shaders")}>
            <div className="grid gap-1.5 text-[12.5px]">
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={s.shd} onChange={(e) => set("shd", e.target.checked)} />
                {t("convert.makeShd")}
              </label>
              <label className={cn("flex items-center gap-2", !s.shd && "opacity-50")}>
                <input type="checkbox" disabled={!s.shd} checked={s.overwriteShd} onChange={(e) => set("overwriteShd", e.target.checked)} />
                {t("convert.overwriteShd")}
              </label>
            </div>
          </Field>

          <Field
            label={t("convert.script")}
            action={
              <button className="cursor-default text-[11px] text-muted-foreground hover:text-foreground" onClick={() => void loadScript()}>
                {t("convert.loadScript")}
              </button>
            }
          >
            <textarea
              rows={5}
              spellCheck={false}
              value={s.script}
              onChange={(e) => set("script", e.target.value)}
              placeholder={"[Hierarchy]\nsteer_rot = -90 180 0"}
              className="w-full rounded-lg border border-input bg-transparent p-2 font-mono text-[11.5px] placeholder:text-faint"
            />
            {script && script !== "ok" && (
              <p className="mt-1 text-[11.5px] text-destructive">{t(script === "noSection" ? "convert.scriptNoSection" : "convert.scriptNoParts")}</p>
            )}
          </Field>
        </fieldset>

        {error && <p className="select-text text-[12px] text-destructive">{error}</p>}

        <Button disabled={running || chosen.length === 0 || (!!script && script !== "ok")} onClick={() => void run()}>
          {running ? <Loader2 className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
          {chosen.length > 1 ? t("convert.convertAll", { count: chosen.length }) : t("convert.convert")}
        </Button>
      </section>

      <section className={cn("relative flex min-w-0 flex-1 flex-col", over && "bg-primary-tint")}>
        <div className="flex flex-none items-center gap-2 px-6 pb-2.5 pt-4">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("convert.files")}</h2>
          <div className="ml-auto flex items-center gap-2">
            {doneCount > 0 && outFolder && (
              <Button size="sm" variant="ghost" onClick={() => void revealInExplorer(jobs!.find((j) => j.status.state === "done")!.output)}>
                <FolderOpen className="size-3.5" /> {t("convert.openFolder")}
              </Button>
            )}
            {files.length > 0 && (
              <Button
                size="sm"
                variant="ghost"
                disabled={running}
                onClick={() => {
                  setFiles([]);
                  setOff(new Set());
                  setJobs(null);
                  layoutTouched.current = false;
                }}
              >
                <X className="size-3.5" /> {t("convert.clear")}
              </Button>
            )}
            <Button size="sm" variant="outline" disabled={running} onClick={() => void pick(false)}>
              <FilePlus className="size-3.5" /> {t("convert.addFiles")}
            </Button>
            <Button size="sm" variant="outline" disabled={running} onClick={() => void pick(true)}>
              <FolderPlus className="size-3.5" /> {t("convert.addFolder")}
            </Button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
          {files.length === 0 ? (
            <div
              className={cn(
                "flex h-full min-h-[240px] flex-col items-center justify-center gap-2 border border-dashed text-[12.5px] text-muted-foreground",
                over ? "border-primary text-foreground" : "border-border",
              )}
            >
              <FileBox className="size-6" />
              {t("convert.drop")}
            </div>
          ) : (
            <ul className="border border-border bg-card text-[12px]">
              {(jobs
                ? jobs.map((j) => ({ path: j.input, kind: j.shadow ? "shadow" : "model", job: j }) as const)
                : files.map((f) => ({ path: f.path, kind: f.kind, job: null }))
              ).map((row) => (
                <Row
                  key={row.path}
                  path={row.path}
                  kind={row.kind}
                  job={row.job}
                  checked={row.kind === "params" ? undefined : !off.has(row.path)}
                  disabled={running}
                  onToggle={() =>
                    setOff((prev) => {
                      const next = new Set(prev);
                      if (!next.delete(row.path)) next.add(row.path);
                      return next;
                    })
                  }
                />
              ))}
            </ul>
          )}
        </div>
      </section>
    </div>
  );
}

function Field({ label, action, children }: { label: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between">
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">{label}</span>
        {action}
      </div>
      {children}
    </div>
  );
}

function Row({
  path,
  kind,
  job,
  checked,
  disabled,
  onToggle,
}: {
  path: string;
  kind: Found["kind"];
  job: Job | null;
  checked?: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  const t = useT();
  const st = job?.status;
  const outcome = st?.state === "done" ? st.outcome : null;
  const report = outcome?.report ?? null;
  return (
    <li className={cn("border-b border-border/60 px-3 py-2 last:border-b-0", checked === false && !job && "opacity-50")}>
      <div className="flex items-center gap-3">
        {job ? (
          st?.state === "working" ? (
            <Loader2 className="size-3.5 flex-none animate-spin text-muted-foreground" />
          ) : report ? (
            <CheckCircle2 className="size-3.5 flex-none text-success" />
          ) : outcome ? (
            <TriangleAlert className="size-3.5 flex-none text-destructive" />
          ) : (
            <span className="size-3.5 flex-none rounded-full border border-border" />
          )
        ) : checked !== undefined ? (
          <input type="checkbox" checked={checked} disabled={disabled} onChange={onToggle} />
        ) : (
          <span className="size-3.5 flex-none" />
        )}
        <span className="min-w-0 flex-1 truncate font-mono" title={job ? `${path} → ${job.output}` : path}>
          {nameOf(path)}
        </span>
        {kind !== "model" && (
          <span className="flex-none rounded-full bg-muted px-2 py-0.5 text-[10.5px] text-muted-foreground">
            {t(kind === "shadow" ? "convert.shadow" : "convert.params")}
          </span>
        )}
        {st?.state === "working" && st.total > 0 && (
          <span className="flex-none tabular-nums text-muted-foreground">
            {st.done}/{st.total}
          </span>
        )}
        {report && (
          <>
            <span className="flex-none text-muted-foreground">{mb(report.bytes)}</span>
            <span className="flex-none tabular-nums text-muted-foreground">{(report.millis / 1000).toFixed(1)} s</span>
            <Button size="sm" variant="ghost" className="h-6 px-1.5" onClick={() => void revealInExplorer(job!.output)}>
              <FolderOpen className="size-3.5" />
            </Button>
          </>
        )}
      </div>
      {st?.state === "working" && st.total > 0 && <Progress className="mt-2" value={(st.done / st.total) * 100} />}
      {outcome?.error && <p className="mt-1 select-text pl-[26px] text-destructive">{outcome.error}</p>}
      {outcome && !report && outcome.shd.length > 0 && (
        <div className="mt-1 grid gap-0.5 pl-[26px] text-[11.5px] text-muted-foreground">
          {outcome.shd.map((line) => (
            <p key={line} className="select-text truncate">
              {line}
            </p>
          ))}
        </div>
      )}
      {report && (
        <div className="mt-1 grid gap-0.5 pl-[26px] text-[11.5px] text-muted-foreground">
          <p className="truncate" title={job!.output}>
            {nameOf(job!.output)}
            {outcome!.hrc.length > 0 && ` · ${outcome!.hrc.join(", ")}`}
            {" · "}
            {report.objects.map((o) => `${o.name || t("convert.mainObject")} ${o.triangles.toLocaleString()}`).join(" · ")}
          </p>
          {outcome!.shd.map((line) => (
            <p key={line} className="select-text truncate">
              {line}
            </p>
          ))}
          {report.warnings.map((w, i) => (
            <p key={i} className="select-text text-amber-600 dark:text-amber-400">
              {w}
            </p>
          ))}
        </div>
      )}
    </li>
  );
}
