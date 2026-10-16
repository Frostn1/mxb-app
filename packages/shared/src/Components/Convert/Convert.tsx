import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { CheckCircle2, FileBox, FileCode2, FilePlus, FolderOpen, FolderPlus, Layers, Loader2, Play, TriangleAlert, X } from "lucide-react";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Progress } from "../ui/progress";
import { Segmented } from "../ui/segmented";
import {
  checkScript,
  convertFiles,
  isBike,
  keptScripts,
  nameOf,
  onDone,
  onProgress,
  onSourceChanged,
  unwatchSources,
  watchSources,
  outputPaths,
  readText,
  rememberScript,
  removeScript,
  saveScript,
  scanPaths,
  scriptSettings,
  type ConvertOptions,
  type ConvertReport,
  type Found,
  type HrcMode,
  type Outcome,
  type SavedScript,
} from "../../api/convert";
import { useT } from "../../i18n/context";
import { ConvertPreview, WrittenFiles } from "./ConvertResult";

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
  /** The file the script came from, when it was dropped or loaded rather than typed. */
  scriptName: string;
  overwriteShd: boolean;
  /** A bike's file: `model.edf`, the name MX Bikes loads, or the FBX's own name as before. */
  naming: "model" | "fbx";
  /** A name for the .edf when one model is written; empty keeps the usual name. */
  outName: string;
  hrc: HrcMode;
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
  scriptName: "",
  overwriteShd: false,
  naming: "model",
  outName: "",
  hrc: "ifNone",
};

function load(key: string): Settings {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(key) ?? "{}") };
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
 * same converter and the same options as mxbsecure.com/convert, without the upload. Frost's
 * Studio shows it as a tab and Frost Converter as its whole window.
 *
 * `active`: drops land here while it is on screen. `storageKey`: where the settings are kept.
 * `opened`: paths to start with, as files dropped on an app's icon.
 */
export default function Convert({ active, storageKey, opened }: { active: boolean; storageKey: string; opened?: string[] }) {
  const t = useT();
  const [s, setS] = useState<Settings>(() => load(storageKey));
  const [files, setFiles] = useState<Found[]>([]);
  const [off, setOff] = useState<Set<string>>(new Set());
  const [jobs, setJobs] = useState<Job[] | null>(null);
  const [running, setRunning] = useState(false);
  /** Which converted model the result below the list is about, by its FBX. */
  const [picked, setPicked] = useState<string | null>(null);
  const [runs, setRuns] = useState(0);
  const [over, setOver] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const layoutTouched = useRef(false);
  const [view, setView] = useState<"convert" | "preview">("convert");
  const [watch, setWatch] = useState(false);
  const [watchPaths, setWatchPaths] = useState<string[]>([]);
  /** The last conversion that worked: what the Preview shows, kept while the next one runs. */
  const [last, setLast] = useState<{ key: string; output: string; outcome: Outcome } | null>(null);
  const redo = useRef(false);
  const [saved, setSaved] = useState<SavedScript[]>([]);

  // The last script used comes back with the app, and the saved ones are listed.
  useEffect(() => {
    let alive = true;
    keptScripts()
      .then((k) => {
        if (!alive) return;
        setSaved(k.saved);
        if (k.last) setS((p) => (p.script.trim() ? p : { ...p, script: k.last!.text, scriptName: k.last!.name }));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  const keep = (name: string, text: string) => {
    rememberScript(name, text).then((k) => setSaved(k.saved)).catch(() => {});
  };

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(s));
    } catch {
      // Blocked storage: the settings just aren't remembered.
    }
  }, [s, storageKey]);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((p) => ({ ...p, [k]: v }));

  const models = useMemo(() => files.filter((f) => f.kind !== "params"), [files]);
  const chosen = useMemo(() => models.filter((f) => !off.has(f.path)), [models, off]);
  // The name field is for one .edf; a bike's shadow keeps the name the game loads.
  const single = chosen.filter((f) => f.kind !== "shadow").length === 1;
  const script = s.script.trim() ? checkScript(s.script, s.layout) : null;
  const settings = s.script.trim() ? scriptSettings(s.script) : [];

  // A dropped export script goes to its slot, not the list: it flies there from the drop, then
  // the slot lights up for a moment.
  const slotRef = useRef<HTMLDivElement>(null);
  const [fly, setFly] = useState<{ name: string; x: number; y: number; dx: number; dy: number; go: boolean } | null>(null);
  const [lit, setLit] = useState(false);
  const light = () => {
    setLit(true);
    window.setTimeout(() => setLit(false), 900);
  };
  const toSlot = (name: string, at?: { x: number; y: number }) => {
    const slot = slotRef.current?.getBoundingClientRect();
    if (slot) slotRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    const still = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (!slot || !at || still) return light();
    setFly({ name, x: at.x, y: at.y, dx: slot.left + 12 - at.x, dy: slot.top + 8 - at.y, go: false });
    requestAnimationFrame(() => requestAnimationFrame(() => setFly((f) => f && { ...f, go: true })));
  };

  const add = async (paths: string[], at?: { x: number; y: number }) => {
    if (running || paths.length === 0) return;
    setView("convert");
    const found = await scanPaths(paths);
    const scripts = found.filter((f) => f.kind === "params");
    const models = found.filter((f) => f.kind !== "params");
    if (models.length) setJobs(null);
    setFiles((prev) => {
      const seen = new Set(prev.map((f) => f.path.toLowerCase()));
      return [...prev, ...models.filter((f) => !seen.has(f.path.toLowerCase()))];
    });
    const last = scripts[scripts.length - 1];
    if (last) {
      const text = await readText(last.path);
      setS((p) => ({ ...p, script: text, scriptName: nameOf(last.path) }));
      keep(nameOf(last.path), text);
      toSlot(nameOf(last.path), at);
    }
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
          const r = window.devicePixelRatio || 1;
          void addRef.current(e.payload.paths, { x: e.payload.position.x / r, y: e.payload.position.y / r });
        } else setOver(e.payload.type === "over" || e.payload.type === "enter");
      })
      .then((f) => (gone ? f() : (un = f)));
    return () => {
      gone = true;
      un?.();
    };
  }, [active]);

  useEffect(() => {
    if (opened?.length) void addRef.current(opened);
  }, [opened]);

  // A bike defaults to Parts, from the first model's own top-level names, until Layout is
  // picked by hand.
  const firstPath = (chosen.find((f) => f.kind === "model") ?? chosen[0])?.path;
  useEffect(() => {
    if (!firstPath || layoutTouched.current) return;
    let alive = true;
    isBike(firstPath)
      .then((bike) => alive && !layoutTouched.current && set("layout", bike ? "parts" : "whole"))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [firstPath]);

  const pick = async (directory: boolean) => {
    const p = await openDialog({
      multiple: true,
      directory,
      filters: directory ? undefined : [{ name: "FBX", extensions: ["fbx", "ini", "params", "txt"] }],
    });
    void add(Array.isArray(p) ? p : p ? [p] : []);
  };
  const pickFolder = async () => {
    const p = await openDialog({ directory: true, defaultPath: s.folder || undefined });
    if (typeof p === "string") set("folder", p);
  };
  const loadScript = async () => {
    const p = await openDialog({ filters: [{ name: "Script", extensions: ["ini", "txt", "params"] }] });
    if (typeof p === "string") {
      const text = await readText(p);
      setS((prev) => ({ ...prev, script: text, scriptName: nameOf(p) }));
      keep(nameOf(p), text);
      light();
    }
  };

  const options = async (): Promise<ConvertOptions | string> => {
    const num = (v: string, what: string) => (Number.isFinite(Number(v)) && v.trim() !== "" ? Number(v) : t("convert.notNumber", { what }));
    const scale = num(s.scale, t("convert.scale"));
    const merge = s.merge ? num(s.mergeDistance, t("convert.merge")) : 0;
    const angle = s.normals === "flat" ? 0 : num(s.angle, t("convert.recalc"));
    for (const v of [scale, merge, s.normals === "recalc" ? angle : 0]) if (typeof v === "string") return v;
    return {
      params: s.script.trim() ? s.script : undefined,
      layout: s.layout,
      main: s.layout === "parts" && s.main.trim() ? s.main.trim() : undefined,
      scale: scale as number,
      mergeDistance: merge as number,
      ...(s.normals === "file" ? { useFileNormals: true } : { recalcNormals: angle as number }),
      lowMemory: !s.fast,
    };
  };

  const saveCurrent = () => {
    const name = s.scriptName.replace(/\.[^.]*$/, "") || "Script";
    saveScript(name, s.script).then((k) => setSaved(k.saved)).catch((e) => setError(String(e)));
  };
  const pickSaved = (x: SavedScript) => {
    setS((p) => ({ ...p, script: x.text, scriptName: x.name }));
    keep(x.name, x.text);
    light();
  };

  const run = async (auto = false) => {
    if (running || chosen.length === 0) return;
    setError(null);
    keep(s.scriptName, s.script);
    let o: ConvertOptions | string;
    try {
      o = await options();
    } catch (e) {
      o = String(e);
    }
    if (typeof o === "string") return setError(o);
    const list = chosen.map((f) => ({ path: f.path, shadow: f.kind === "shadow" }));
    let outs: string[];
    try {
      outs = await outputPaths(list, s.out === "folder" && s.folder ? s.folder : null, s.layout === "parts" && s.naming === "model", single ? s.outName : null);
    } catch (e) {
      return setError(String(e));
    }
    const batch: Job[] = list.map((m, i) => ({ input: m.path, output: outs[i], shadow: m.shadow, status: { state: "waiting" } }));
    setJobs(batch);
    setWatchPaths(batch.map((j) => j.input));
    setRunning(true);
    const patch = (i: number, status: Status) => setJobs((js) => js && js.map((j, k) => (k === i ? { ...j, status } : j)));
    const unProgress = await onProgress((e) => patch(e.index, { state: "working", done: e.done, total: e.total }));
    const unDone = await onDone((e) => patch(e.index, { state: "done", outcome: e.outcome }));
    try {
      const results = await convertFiles(
        batch.map((j) => ({ input: j.input, output: j.output })),
        o,
        s.hrc,
        s.overwriteShd,
      );
      setRuns((n) => n + 1);
      setJobs((js) => js && js.map((j, i) => ({ ...j, status: { state: "done", outcome: results[i] } })));
      if (!auto && results.some((r) => r.report)) setView("preview");
    } catch (e) {
      setError(String(e));
      setJobs(null);
    } finally {
      unProgress();
      unDone();
      setRunning(false);
      // A source saved while this ran is converted next.
      if (redo.current) {
        redo.current = false;
        window.setTimeout(() => void runRef.current(true), 0);
      }
    }
  };
  const runRef = useRef(run);
  runRef.current = run;
  const runningRef = useRef(running);
  runningRef.current = running;

  const converted = (jobs ?? []).filter((j): j is Job & { status: { state: "done"; outcome: Outcome } } => j.status.state === "done" && !!j.status.outcome.report);
  const shown = running ? undefined : (converted.find((j) => j.input === picked) ?? converted[0]);
  useEffect(() => {
    if (shown) setLast({ key: `${runs}:${shown.output}`, output: shown.output, outcome: shown.status.outcome });
    else if (!jobs && !running) setLast(null);
  }, [shown, runs, jobs, running]);

  // Watch: a source saved over converts again and the Preview follows.
  const watchKey = JSON.stringify(watchPaths);
  useEffect(() => {
    if (!watch || watchKey === "[]") return;
    let un: (() => void) | undefined;
    let gone = false;
    watchSources(JSON.parse(watchKey) as string[]).catch((e) => setError(String(e)));
    void onSourceChanged(() => {
      if (runningRef.current) redo.current = true;
      else void runRef.current(true);
    }).then((f) => (gone ? f() : (un = f)));
    return () => {
      gone = true;
      un?.();
      void unwatchSources().catch(() => {});
    };
  }, [watch, watchKey]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {fly && (
        <div
          className="pointer-events-none fixed z-50 flex items-center gap-1.5 rounded-lg border border-border bg-card px-2.5 py-1.5 text-[12px] shadow-md transition-[transform,opacity] duration-500 ease-out"
          style={{ left: fly.x, top: fly.y, transform: fly.go ? `translate(${fly.dx}px, ${fly.dy}px) scale(0.9)` : "translate(-50%, -50%)", opacity: fly.go ? 0.2 : 1 }}
          onTransitionEnd={(e) => {
            if (e.propertyName !== "transform") return;
            setFly(null);
            light();
          }}
        >
          <FileCode2 className="size-3.5 text-muted-foreground" />
          <span className="font-mono">{fly.name}</span>
        </div>
      )}
      <div className="flex flex-none items-center gap-3 border-b border-border px-6 py-2">
        <Segmented
          size="sm"
          value={view}
          onChange={setView}
          options={[
            { value: "convert", label: t("convert.tabConvert") },
            { value: "preview", label: t("convert.tabPreview") },
          ]}
        />
        {running && (
          <span className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" /> {t("convert.converting")}
          </span>
        )}
      </div>
      <div className="min-h-0 flex-1">
        {view === "preview" ? (
          <ConvertPreview
            last={last}
            running={running}
            watching={watch}
            canWatch={!!last}
            onWatch={setWatch}
            onConvert={() => setView("convert")}
          />
        ) : (
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

          {s.layout === "parts" && (
            <>
              <Field label={t("convert.fileName")}>
                <Segmented
                  size="sm"
                  value={s.naming}
                  onChange={(v) => set("naming", v)}
                  options={[
                    { value: "model", label: "model.edf" },
                    { value: "fbx", label: t("convert.fbxName") },
                  ]}
                />
              </Field>
            </>
          )}

          {single && (
            <Field label={t("convert.outName")}>
              <Input className="h-8" value={s.outName} placeholder="model.edf" onChange={(e) => set("outName", e.target.value)} />
              <p className="mt-1 text-[11.5px] text-muted-foreground">{t("convert.outNameHint")}</p>
            </Field>
          )}

          <div ref={slotRef} className={cn("-mx-2 rounded-lg px-2 py-1 ring-1 ring-transparent transition-[background-color,box-shadow] duration-700", lit && "bg-primary/[0.07] ring-primary/40")}>
          <Field
            label={t("convert.script")}
            action={
              <span className="flex gap-3">
                {s.script.trim() && (
                  <button className="cursor-default text-[11px] text-muted-foreground hover:text-foreground" onClick={saveCurrent}>
                    {t("convert.saveScript")}
                  </button>
                )}
                <button className="cursor-default text-[11px] text-muted-foreground hover:text-foreground" onClick={() => void loadScript()}>
                  {t("convert.loadScript")}
                </button>
              </span>
            }
          >
            <textarea
              rows={5}
              spellCheck={false}
              value={s.script}
              onChange={(e) => setS((p) => ({ ...p, script: e.target.value, scriptName: e.target.value.trim() ? p.scriptName : "" }))}
              placeholder={"[Hierarchy]\nsteer_rot = -90 180 0"}
              className="w-full rounded-lg border border-input bg-transparent p-2 font-mono text-[11.5px] placeholder:text-faint"
            />
            {saved.length > 0 && (
              <div className="mt-1.5 flex flex-wrap gap-1.5" aria-label={t("convert.savedScripts")}>
                {saved.map((x) => (
                  <span key={x.name} className={cn("flex items-center rounded-md border border-input text-[11.5px]", x.text === s.script && "border-primary/50 bg-primary/[0.07]")}>
                    <button className="cursor-default px-2 py-0.5 hover:text-foreground" title={t("convert.useScript")} onClick={() => pickSaved(x)}>
                      {x.name}
                    </button>
                    <button
                      className="cursor-default px-1 py-0.5 text-muted-foreground hover:text-destructive"
                      title={t("convert.removeScript")}
                      aria-label={t("convert.removeScript")}
                      onClick={() => void removeScript(x.name).then((k) => setSaved(k.saved)).catch(() => {})}
                    >
                      <X className="size-3" />
                    </button>
                  </span>
                ))}
              </div>
            )}
            {script && script !== "ok" && (
              <p className="mt-1 text-[11.5px] text-destructive">{t(script === "noSection" ? "convert.scriptNoSection" : "convert.scriptNoParts")}</p>
            )}
            {s.scriptName && s.script.trim() && (
              <p className="mt-1 flex min-w-0 items-center gap-1.5 text-[11.5px] text-muted-foreground" title={s.scriptName}>
                <FileCode2 className="size-3.5 flex-none" />
                <span className="truncate">
                  <span className="font-mono">{s.scriptName}</span>
                  {settings.length > 0 && ` · ${settings.join(", ")}`}
                </span>
              </p>
            )}
          </Field>
          </div>

          <details className="group rounded-lg border border-border px-3 py-2">
            <summary className="cursor-default select-none text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("convert.more")}</summary>
            <div className="mt-3 grid gap-4">
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

              {s.layout === "parts" && (
                <>
                            <Field label={t("convert.hrc")}>
                              <Segmented
                                size="sm"
                                value={s.hrc}
                                onChange={(v) => set("hrc", v)}
                                options={[
                                  { value: "always", label: t("convert.hrcAlways") },
                                  { value: "ifNone", label: t("convert.hrcIfNone") },
                                  { value: "never", label: t("convert.hrcNever") },
                                ]}
                              />
                            </Field>
                </>
              )}

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
                            <p className="text-[11.5px] text-muted-foreground">{t("convert.shdAlways")}</p>
                            <label className="flex items-center gap-2">
                              <input type="checkbox" checked={s.overwriteShd} onChange={(e) => set("overwriteShd", e.target.checked)} />
                              {t("convert.overwriteShd")}
                            </label>
                          </div>
                        </Field>
            </div>
          </details>
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
                  selected={shown?.input === row.path}
                  onSelect={() => setPicked(row.path)}
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
          {shown && <WrittenFiles outcome={shown.status.outcome} />}
        </div>
      </section>

          </div>
        )}
      </div>
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

/** What the file leaves out and why, laid out like the MXB App's skipped list. */
function Skipped({ items }: { items: { name: string; reason: string }[] }) {
  const t = useT();
  return (
    <div className="mt-1.5 rounded-lg border border-border bg-card/40 px-3.5 py-2.5">
      <div className="flex items-center gap-2 text-[11.5px] font-semibold text-muted-foreground">
        <Layers className="size-3.5" />
        {t("convert.skipped", { count: items.length })}
      </div>
      <ul className="mt-1 space-y-0.5">
        {items.map((s, i) => (
          <li key={i} className="select-text text-[11px] text-muted-foreground">
            <span className="font-mono">{s.name}</span> — {s.reason}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Textures written with a flag: `visor.tga: alpha blend`. */
function useFlagged() {
  const t = useT();
  return (report: ConvertReport) =>
    report.textures
      .filter((x) => (x.flags & 3) !== 0)
      .map((x) => {
        const what = [x.flags & 1 && t("convert.colourKey"), x.flags & 2 && t("convert.alphaBlend")].filter(Boolean);
        return `${x.name}: ${what.join(", ")}`;
      });
}

function Row({
  path,
  kind,
  job,
  checked,
  disabled,
  selected,
  onSelect,
  onToggle,
}: {
  path: string;
  kind: Found["kind"];
  job: Job | null;
  checked?: boolean;
  disabled: boolean;
  /** This is the converted model the result below the list is showing. */
  selected: boolean;
  onSelect: () => void;
  onToggle: () => void;
}) {
  const t = useT();
  const flagged = useFlagged();
  const st = job?.status;
  const outcome = st?.state === "done" ? st.outcome : null;
  const report = outcome?.report ?? null;
  return (
    <li
      className={cn("border-b border-border/60 px-3 py-2 last:border-b-0", checked === false && !job && "opacity-50", selected && report && "bg-muted/40")}
      onClick={report ? onSelect : undefined}
    >
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
          </>
        )}
      </div>
      {st?.state === "working" && st.total > 0 && <Progress className="mt-2" value={(st.done / st.total) * 100} />}
      {outcome?.error && <p className="mt-1 select-text pl-[26px] text-destructive">{outcome.error}</p>}
      {outcome && !report && outcome.shd.length + outcome.notes.length > 0 && (
        <div className="mt-1 grid gap-0.5 pl-[26px] text-[11.5px] text-muted-foreground">
          {outcome.shd.map((line) => (
            <p key={line} className="select-text truncate">
              {line}
            </p>
          ))}
          {outcome.notes.map((line, i) => (
            <p key={i} className="select-text text-amber-600 dark:text-amber-400">
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
          {flagged(report).map((line) => (
            <p key={line} className="select-text truncate">
              {line}
            </p>
          ))}
          {[...outcome!.notes, ...report.warnings].map((w, i) => (
            <p key={i} className="select-text text-amber-600 dark:text-amber-400">
              {w}
            </p>
          ))}
          {report.skipped.length > 0 && <Skipped items={report.skipped} />}
        </div>
      )}
    </li>
  );
}
