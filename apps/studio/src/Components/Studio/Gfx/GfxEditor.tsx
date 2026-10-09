import { useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FolderOpen, Loader2, Save } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@frost/shared/Components/ui/select";
import { ModelViewer } from "@frost/shared/Components/Viewer/ModelViewer";
import type { BikeModel, EdfNode, Vec3 } from "@frost/shared/types";
import { PaneActive, UnsavedRegistry } from "../../Shell/ContextBar";
import {
  AXES,
  LEVERS,
  POINTS,
  changes,
  fmt,
  gfxOpen,
  gfxPreview,
  gfxSave,
  mirrorGrip,
  outOfStep,
  type GfxFile,
  type ReloadOutcome,
} from "@/api/gfx";
import { useT, type TKey } from "@/i18n";
import { GfxMarkers, type Marker } from "./GfxMarkers";

const COLOR = { steer: "#7cb8ff", chassis: "#ffb020", lever: "#b8b8b8" };
const REMEMBER = "studio.gfx";

type Status = { kind: "saved"; reload: ReloadOutcome | null } | { kind: "error"; text: string } | null;

/**
 * Gfx: a bike's gfx.cfg on the model. Grips, chain and exhaust are dots to drag; levers show
 * where their node sits. Save writes the file (main and cockpit copies) and asks FrostMod to
 * re-read it.
 */
export default function GfxEditor() {
  const t = useT();
  const active = useContext(PaneActive);
  const { register } = useContext(UnsavedRegistry);
  const [path, setPath] = useState<string | null>(null);
  const [file, setFile] = useState<GfxFile | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [model, setModel] = useState<BikeModel | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<Status>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [over, setOver] = useState(false);
  const [mirror, setMirror] = useState(() => {
    try {
      return localStorage.getItem(`${REMEMBER}.mirror`) !== "0";
    } catch {
      return true;
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(`${REMEMBER}.mirror`, mirror ? "1" : "0");
    } catch {
      // Not remembered.
    }
  }, [mirror]);

  const edits = useMemo(() => changes(values, file?.fields ?? {}), [values, file]);
  const dirty = edits.length > 0;

  const load = useCallback(async (p: string) => {
    setStatus(null);
    setLoading(true);
    setModel(null);
    try {
      const f = await gfxOpen(p);
      setPath(p);
      setFile(f);
      setValues(f.fields);
      setSelected(null);
      try {
        localStorage.setItem(`${REMEMBER}.last`, p);
      } catch {
        // Not remembered.
      }
      try {
        setModel(await gfxPreview(p));
      } catch (e) {
        setStatus({ kind: "error", text: String(e) });
      }
    } catch (e) {
      setStatus({ kind: "error", text: String(e) });
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(async (): Promise<boolean> => {
    if (!path || !dirty) return true;
    setSaving(true);
    setStatus(null);
    try {
      const r = await gfxSave(path, edits);
      setFile(r.file);
      setValues(r.file.fields);
      setStatus({ kind: "saved", reload: r.reload });
      return true;
    } catch (e) {
      setStatus({ kind: "error", text: String(e) });
      return false;
    } finally {
      setSaving(false);
    }
  }, [path, dirty, edits]);

  const live = useRef({ dirty, save });
  live.current = { dirty, save };
  useEffect(() => register({ dirty: () => live.current.dirty, save: () => live.current.save() }), [register]);

  const pick = async () => {
    const p = await openDialog({ directory: true });
    if (typeof p === "string") void load(p);
  };

  // Drops land on the whole pane while it is on screen.
  const loadRef = useRef(load);
  loadRef.current = load;
  useEffect(() => {
    if (!active) return;
    let un: (() => void) | undefined;
    let gone = false;
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          setOver(false);
          const first = e.payload.paths[0];
          if (first) void loadRef.current(first);
        } else setOver(e.payload.type === "over" || e.payload.type === "enter");
      })
      .then((f) => (gone ? f() : (un = f)));
    return () => {
      gone = true;
      un?.();
    };
  }, [active]);

  const setField = (k: string, v: string) => {
    setStatus(null);
    setValues((prev) => {
      const next = { ...prev, [k]: v };
      const grip = /^steer\/(leftgrip|rightgrip)\/pos\//.exec(k)?.[1] as "leftgrip" | "rightgrip" | undefined;
      return mirror && grip ? mirrorGrip(next, grip) : next;
    });
  };

  const movePoint = (id: string, at: Vec3) => {
    const pt = POINTS.find((p) => p.id === id);
    if (!pt) return;
    setStatus(null);
    setValues((prev) => {
      const next = { ...prev };
      (["x", "y", "z"] as const).forEach((k, i) => (next[`${pt.path}/${k}`] = fmt(at[i])));
      return mirror && (id === "leftgrip" || id === "rightgrip") ? mirrorGrip(next, id) : next;
    });
  };

  const num = (k: string) => {
    const v = values[k];
    const n = v === undefined || v.trim() === "" ? NaN : Number(v);
    return Number.isFinite(n) ? n : null;
  };
  const vec = (base: string): Vec3 | null => {
    const x = num(`${base}/x`);
    const y = num(`${base}/y`);
    const z = num(`${base}/z`);
    return x === null || y === null || z === null ? null : [x, y, z];
  };

  const linkedEngine = values["chassis/chain/engine/link_obj"] !== undefined;
  const nodes = model?.nodes ?? null;
  const rig = model?.rig ?? null;

  const markers = useMemo<Marker[]>(() => {
    const out: Marker[] = [];
    for (const p of POINTS) {
      if (p.id === "engine" && linkedEngine) continue;
      const at = vec(p.path);
      if (!at) continue;
      const dir = p.id === "exhaust" ? vec("chassis/exhaust/dir") ?? undefined : undefined;
      out.push({ id: p.id, at, frame: p.frame, dir, color: COLOR[p.frame] });
    }
    for (const l of LEVERS) {
      const name = values[`${l.path}/name`];
      const view = name && nodes ? leverPoint(nodes, name) : null;
      if (view) out.push({ id: l.id, at: [0, 0, 0], frame: "chassis", view, fixed: true, color: COLOR.lever });
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [values, nodes, linkedEngine]);

  const textures = useMemo(() => model?.paints[0]?.textures ?? [], [model]);
  const differs = file ? outOfStep(file.fields) : [];
  const has = (k: string) => values[k] !== undefined;
  const levers = LEVERS.filter((l) => has(`${l.path}/axis`) || has(`${l.path}/maxrot`));

  return (
    <div className="flex h-full min-h-0">
      <section className="flex w-[340px] flex-none flex-col gap-4 overflow-y-auto border-r border-border px-6 py-4">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("gfx.title")}</h2>
        {file && (
          <fieldset disabled={saving} className="contents">
            <Group
              label={t("gfx.grips")}
              action={
                <label className="flex items-center gap-2 text-[11.5px] text-muted-foreground">
                  {t("gfx.mirror")}
                  <Switch checked={mirror} onCheckedChange={setMirror} />
                </label>
              }
            >
              {(["leftgrip", "rightgrip"] as const).map((g) => (
                <Xyz
                  key={g}
                  label={t(`gfx.${g}` as TKey)}
                  base={`steer/${g}/pos`}
                  values={values}
                  onChange={setField}
                  active={selected === g}
                  onFocus={() => setSelected(g)}
                />
              ))}
            </Group>

            {levers.length > 0 && (
              <Group label={t("gfx.levers")}>
                {levers.map((l) => (
                  <div key={l.id} className={cn("grid gap-1", selected === l.id && "text-foreground")} onFocus={() => setSelected(l.id)}>
                    <span className="text-[12px]">{t(`gfx.${l.id}` as TKey)}</span>
                    <div className="grid grid-cols-2 gap-2">
                      <Select value={values[`${l.path}/axis`] ?? ""} onValueChange={(v) => setField(`${l.path}/axis`, v)}>
                        <SelectTrigger className="h-8" aria-label={t("gfx.axis")}>
                          <SelectValue placeholder={t("gfx.axis")} />
                        </SelectTrigger>
                        <SelectContent>
                          {AXES.map((a) => (
                            <SelectItem key={a} value={a}>
                              {a}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <Labeled label="°">
                        <Input
                          className="h-8 pr-6 text-right"
                          inputMode="decimal"
                          aria-label={t("gfx.maxrot")}
                          value={values[`${l.path}/maxrot`] ?? ""}
                          onChange={(e) => setField(`${l.path}/maxrot`, e.target.value)}
                        />
                      </Labeled>
                    </div>
                  </div>
                ))}
              </Group>
            )}

            <Group label={t("gfx.chain")}>
              {!linkedEngine && (
                <Xyz
                  label={t("gfx.engine")}
                  base="chassis/chain/engine"
                  values={values}
                  onChange={setField}
                  active={selected === "engine"}
                  onFocus={() => setSelected("engine")}
                />
              )}
              <Xyz label={t("gfx.chainPos")} base="chassis/chain/pos" values={values} onChange={setField} />
            </Group>

            <Group label={t("gfx.exhaust")}>
              <Xyz
                label={t("gfx.pos")}
                base="chassis/exhaust/pos"
                values={values}
                onChange={setField}
                active={selected === "exhaust"}
                onFocus={() => setSelected("exhaust")}
              />
              <Xyz label={t("gfx.dir")} base="chassis/exhaust/dir" values={values} onChange={setField} />
            </Group>

            <Group label={t("gfx.rider")}>
              <Xyz label={t("gfx.riderOffset")} base="rider/xform" values={values} onChange={setField} />
            </Group>
          </fieldset>
        )}

        {differs.length > 0 && !dirty && <p className="text-[12px] text-amber-600 dark:text-amber-400">{t("gfx.cockpitDiffers")}</p>}
        {status?.kind === "error" && <p className="select-text text-[12px] text-destructive">{status.text}</p>}
        {status?.kind === "saved" && (
          <p className="text-[12px] text-muted-foreground">
            {t(status.reload === "signaled" ? "gfx.reloaded" : status.reload === "not_running" ? "gfx.notRunning" : "gfx.saved")}
          </p>
        )}

        {file && (
          <Button disabled={saving || !dirty} onClick={() => void save()}>
            {saving ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
            {t("gfx.save")}
          </Button>
        )}
      </section>

      <section className={cn("relative flex min-w-0 flex-1 flex-col", over && "bg-primary-tint")}>
        <div className="flex flex-none items-center gap-2 px-6 pb-2.5 pt-4">
          <h2 className="min-w-0 truncate text-[11px] font-semibold uppercase tracking-[0.09em] text-faint" title={file?.gfxPath}>
            {file?.label ?? t("gfx.model")}
          </h2>
          {file?.variant && file.active && (
            <span className="flex-none rounded-full bg-muted px-2 py-0.5 text-[10.5px] text-muted-foreground">{t("gfx.inUse")}</span>
          )}
          <div className="ml-auto flex items-center gap-2">
            <Button size="sm" variant="outline" disabled={saving} onClick={() => void pick()}>
              <FolderOpen className="size-3.5" /> {t("gfx.open")}
            </Button>
          </div>
        </div>
        <div className="relative min-h-0 flex-1 px-6 pb-5">
          {!file && !loading ? (
            <div
              className={cn(
                "flex h-full min-h-[240px] flex-col items-center justify-center gap-2 border border-dashed text-[12.5px] text-muted-foreground",
                over ? "border-primary text-foreground" : "border-border",
              )}
            >
              <FolderOpen className="size-6" />
              {t("gfx.drop")}
            </div>
          ) : (
            <div className="relative h-full min-h-[240px] border border-border">
              <ModelViewer
                mode="bike"
                nodes={nodes}
                rig={rig}
                textures={textures}
                loading={loading || !model}
                noStandIn
                grounded={false}
                className="absolute inset-0"
                bikeOverlay={
                  model ? (
                    <GfxMarkers markers={markers} rig={rig} selected={selected} onSelect={setSelected} onMove={movePoint} />
                  ) : null
                }
              />
              {loading && <Loader2 className="absolute left-1/2 top-1/2 size-5 -translate-x-1/2 -translate-y-1/2 animate-spin text-muted-foreground" />}
              {model && !rig && <p className="absolute left-3 top-3 text-[11.5px] text-muted-foreground">{t("gfx.unassembled")}</p>}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function Group({ label, action, children }: { label: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="grid gap-2.5">
      <div className="flex items-center justify-between">
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">{label}</span>
        {action}
      </div>
      {children}
    </div>
  );
}

function Labeled({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="relative">
      {children}
      <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[11px] text-faint">{label}</span>
    </div>
  );
}

function Xyz({
  label,
  base,
  values,
  onChange,
  active,
  onFocus,
}: {
  label: string;
  base: string;
  values: Record<string, string>;
  onChange: (k: string, v: string) => void;
  active?: boolean;
  onFocus?: () => void;
}) {
  return (
    <div className="grid gap-1" onFocus={onFocus}>
      <span className={cn("text-[12px]", active ? "text-foreground" : "text-muted-foreground")}>{label}</span>
      <div className="grid grid-cols-3 gap-2">
        {(["x", "y", "z"] as const).map((k) => (
          <Labeled key={k} label={k}>
            <Input
              className={cn("h-8 pr-5 text-right tabular-nums", active && "border-primary/60")}
              inputMode="decimal"
              aria-label={`${label} ${k}`}
              value={values[`${base}/${k}`] ?? ""}
              onChange={(e) => onChange(`${base}/${k}`, e.target.value)}
            />
          </Labeled>
        ))}
      </div>
    </div>
  );
}

/**
 * Where a lever's node sits on the drawn mesh: the middle of the submesh (or node) named
 * after it. `brake_lever` also matches `brake`, the mesh a Blender export hangs under the
 * `brake_lever` empty. Null when the mesh has nothing by that name.
 */
function leverPoint(nodes: EdfNode[], name: string): Vec3 | null {
  const want = name.toLowerCase();
  const stem = want.replace(/_lever$/, "");
  const base = (s: string) => s.toLowerCase().replace(/\.\d+$/, "");
  const hit = (s: string) => {
    const b = base(s);
    return b === want || b === stem;
  };
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  const take = (n: EdfNode, from: number, to: number) => {
    for (let i = from; i < to; i++) {
      const v = n.indices[i] * 3;
      for (let k = 0; k < 3; k++) {
        lo[k] = Math.min(lo[k], n.positions[v + k]);
        hi[k] = Math.max(hi[k], n.positions[v + k]);
      }
    }
  };
  for (const n of nodes) {
    if (hit(n.name)) take(n, 0, n.indices.length);
    else for (const s of n.submeshes) if (hit(s.name)) take(n, s.triStart * 3, (s.triStart + s.triCount) * 3);
  }
  if (!Number.isFinite(lo[0])) return null;
  return [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
}
