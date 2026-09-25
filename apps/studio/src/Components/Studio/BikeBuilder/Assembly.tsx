import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Bike,
  ChevronsDown,
  ChevronsUp,
  FolderOpen,
  Hammer,
  Loader2,
  RotateCcw,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { revealInExplorer } from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import {
  addPlaceholderBike,
  buildBike,
  getAssembly,
  nudge,
  setBuildName,
  setTemplate,
  type AssemblyView,
  type BuildReport,
  type Role,
  type V3,
} from "../../../api/bikebuild";
import Preview3D from "./Preview3D";

/** Nudge steps, metres. */
const STEPS = [0.001, 0.005, 0.01];

/**
 * Phase C to E on one panel: the template the bike is built for, the parts put together on
 * it (in 3D), nudging one, and building the bike folder.
 */
export default function Assembly({
  ready,
  version,
  onChanged,
}: {
  ready: boolean;
  version: number;
  onChanged: () => void;
}) {
  const t = useT();
  const [view, setView] = useState<AssemblyView | null>(null);
  const [selected, setSelected] = useState<Role | null>(null);
  const [step, setStep] = useState(STEPS[1]);
  const [busy, setBusy] = useState<"placeholder" | "template" | "build" | null>(null);
  const [report, setReport] = useState<BuildReport | null>(null);
  const [name, setName] = useState("");

  const load = useCallback(() => {
    getAssembly()
      .then((v) => {
        setView(v);
        setName(v.name);
      })
      .catch((e) => toast.error(t("bike.assemblyFailed"), { description: String(e) }));
  }, [t]);
  useEffect(() => load(), [load, version]);

  async function run<T>(what: typeof busy, f: () => Promise<T>, failed: Parameters<typeof t>[0]) {
    setBusy(what);
    try {
      return await f();
    } catch (e) {
      toast.error(t(failed), { description: String(e) });
      return undefined;
    } finally {
      setBusy(null);
    }
  }

  async function onPlaceholder() {
    if (await run("placeholder", addPlaceholderBike, "bike.placeholderFailed")) onChanged();
  }

  async function onChooseTemplate() {
    const dir = await openDialog({ directory: true, multiple: false, title: t("bike.chooseTemplate") });
    if (typeof dir !== "string") return;
    const v = await run("template", () => setTemplate(dir), "bike.templateFailed");
    if (v) setView(v);
  }

  async function onPlaceholderTemplate() {
    const v = await run("template", () => setTemplate(null), "bike.templateFailed");
    if (v) setView(v);
  }

  async function onNudge(delta: V3 | null) {
    if (!selected) return;
    try {
      setView(await nudge(selected, delta));
    } catch (e) {
      toast.error(t("bike.nudgeFailed"), { description: String(e) });
    }
  }

  async function onBuild() {
    await setBuildName(name).catch(() => undefined);
    const r = await run("build", buildBike, "bike.buildFailed");
    if (r) setReport(r);
  }

  const placed = view?.assembly.placed ?? [];
  const sel = placed.find((p) => p.role === selected) ?? null;
  const mm = (v: number) => `${Math.round(v * 1000)}`;
  // Blender's frame: forward is -Y, up is +Z, the rider's left is +X.
  const moves: { icon: typeof ArrowUp; label: Parameters<typeof t>[0]; d: V3 }[] = [
    { icon: ArrowLeft, label: "bike.forward", d: [0, -step, 0] },
    { icon: ArrowRight, label: "bike.back", d: [0, step, 0] },
    { icon: ArrowUp, label: "bike.up", d: [0, 0, step] },
    { icon: ArrowDown, label: "bike.down", d: [0, 0, -step] },
    { icon: ChevronsUp, label: "bike.left", d: [step, 0, 0] },
    { icon: ChevronsDown, label: "bike.right", d: [-step, 0, 0] },
  ];

  return (
    <>
      <section className="flex flex-col gap-3 border border-border bg-card p-4">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("bike.assemble")}
          </h2>
          <div className="ml-auto flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onClick={onPlaceholder} disabled={!ready || busy !== null}>
              {busy === "placeholder" ? <Loader2 className="size-3.5 animate-spin" /> : <Bike className="size-3.5" />}
              {t("bike.addPlaceholder")}
            </Button>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="text-muted-foreground">{t("bike.template")}</span>
          <span className="font-medium">
            {view?.template.source.kind === "bike" ? view.template.name : t("bike.placeholderTemplate")}
          </span>
          <Button size="sm" variant="ghost" onClick={onChooseTemplate} disabled={busy !== null}>
            <FolderOpen className="size-3.5" />
            {t("bike.chooseTemplate")}
          </Button>
          {view?.template.source.kind === "bike" && (
            <Button size="sm" variant="ghost" onClick={onPlaceholderTemplate} disabled={busy !== null}>
              {t("bike.usePlaceholder")}
            </Button>
          )}
        </div>
        {view?.template.problem && (
          <p className="flex items-center gap-1 text-[12px] text-amber-500">
            <AlertTriangle className="size-3.5" />
            {t("bike.templateProblem", { problem: view.template.problem })}
          </p>
        )}

        {placed.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("bike.assembleEmpty")}</p>
        ) : (
          <>
            <Preview3D
              placed={placed}
              anchors={view?.assembly.anchors ?? {}}
              selected={selected}
              onSelect={setSelected}
              version={version}
            />
            <div className="flex flex-wrap gap-1">
              {placed.map((p) => (
                <Button
                  key={p.role}
                  size="sm"
                  variant={p.role === selected ? "default" : "outline"}
                  onClick={() => setSelected(p.role === selected ? null : p.role)}
                  title={t("bike.snappedBy", { by: t(`bike.by.${p.by === "as modelled" ? "modelled" : p.by}`) })}
                >
                  {t(`bike.role.${p.role}`)}
                  {p.nudge.some((v) => v !== 0) ? " •" : ""}
                </Button>
              ))}
            </div>
            {sel ? (
              <div className="flex flex-wrap items-center gap-2 text-sm">
                <span className="text-muted-foreground">
                  {t("bike.nudgeFor", { role: t(`bike.role.${sel.role}`) })}
                </span>
                {moves.map(({ icon: Icon, label, d }) => (
                  <Button key={label} size="sm" variant="outline" onClick={() => onNudge(d)} title={t(label)}>
                    <Icon className="size-3.5" />
                    {t(label)}
                  </Button>
                ))}
                <select
                  className="h-8 border border-input bg-transparent px-2 text-[12px]"
                  value={step}
                  onChange={(e) => setStep(Number(e.target.value))}
                >
                  {STEPS.map((s) => (
                    <option key={s} value={s}>
                      {mm(s)} mm
                    </option>
                  ))}
                </select>
                <Button size="sm" variant="ghost" onClick={() => onNudge(null)} title={t("bike.resetNudge")}>
                  <RotateCcw className="size-3.5" />
                </Button>
                <span className="font-mono text-[11px] text-muted-foreground">
                  {t("bike.nudged", { x: mm(sel.nudge[0]), y: mm(sel.nudge[1]), z: mm(sel.nudge[2]) })} ·{" "}
                  {t("bike.snappedBy", { by: t(`bike.by.${sel.by === "as modelled" ? "modelled" : sel.by}`) })}
                </span>
              </div>
            ) : (
              <p className="text-[12px] text-muted-foreground">{t("bike.pickToNudge")}</p>
            )}
          </>
        )}
      </section>

      <section className="flex flex-col gap-3 border border-border bg-card p-4">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.build")}</h2>
        <div className="flex flex-wrap items-center gap-2">
          <input
            className="h-8 w-64 border border-input bg-transparent px-2 text-sm"
            placeholder={t("bike.buildNamePlaceholder")}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Button size="sm" onClick={onBuild} disabled={!ready || busy !== null || placed.length === 0}>
            {busy === "build" ? <Loader2 className="size-3.5 animate-spin" /> : <Hammer className="size-3.5" />}
            {t("bike.buildIt")}
          </Button>
        </div>
        {report && (
          <div className="flex flex-col gap-1.5 text-sm">
            <p className="flex items-center gap-2">
              <span className="truncate font-mono text-[11px] text-muted-foreground">{report.folder}</span>
              <Button size="sm" variant="ghost" onClick={() => revealInExplorer(report.folder)}>
                <FolderOpen className="size-3.5" />
                {t("bike.openFolder")}
              </Button>
            </p>
            <p className="font-mono text-[11px]">{report.files.join("  ")}</p>
            <p className="text-[12px] text-muted-foreground">
              {Object.entries(report.tris)
                .map(([g, n]) => `${g} ${n.toLocaleString()} (${t("bike.shadow")} ${report.shadowTris[g] ?? 0})`)
                .join(" · ")}
            </p>
            <p className={report.converted ? "text-[12px] text-emerald-500" : "text-[12px] text-amber-500"}>
              {report.converter}
            </p>
            {report.notes.map((n) => (
              <p key={n} className="text-[12px] text-muted-foreground">
                {n}
              </p>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
