import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Bike, FolderOpen, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import {
  addPlaceholderBike,
  getAssembly,
  nudge,
  setTemplate,
  type AssemblyView,
  type Role,
  type V3,
} from "../../../api/bikebuild";
import Preview3D from "./Preview3D";
import BikePicker from "./BikePicker";
import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ChevronsDown,
  ChevronsUp,
  RotateCcw,
} from "lucide-react";

/** Nudge steps, metres. */
const STEPS = [0.001, 0.005, 0.01];

/**
 * The right panel: what template the bike is built for, the parts snapped onto it in 3D,
 * and nudging one. Split out of the old `Assembly` (which also built the bike) so this panel
 * can sit beside the tray and the slots instead of stacking under them.
 */
export default function PreviewPane({
  version,
  onChanged,
  view,
  setView,
}: {
  version: number;
  onChanged: () => void;
  view: AssemblyView | null;
  setView: (v: AssemblyView) => void;
}) {
  const t = useT();
  const [selected, setSelected] = useState<Role | null>(null);
  const [step, setStep] = useState(STEPS[1]);
  const [busy, setBusy] = useState<"placeholder" | "template" | null>(null);

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

  async function onPickTemplate(path: string | null) {
    const v = await run("template", () => setTemplate(path), "bike.templateFailed");
    if (v) setView(v);
  }

  async function onChooseTemplate() {
    const dir = await openDialog({ directory: true, multiple: false, title: t("bike.chooseTemplate") });
    if (typeof dir === "string") await onPickTemplate(dir);
  }

  async function onNudge(delta: V3 | null) {
    if (!selected) return;
    try {
      setView(await nudge(selected, delta));
    } catch (e) {
      toast.error(t("bike.nudgeFailed"), { description: String(e) });
    }
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
    <section data-dock="right" className="flex h-full min-w-0 flex-col gap-3 overflow-y-auto p-4">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.assemble")}</h2>

      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span className="text-muted-foreground">{t("bike.template")}</span>
        <span className="min-w-0 truncate font-medium">
          {view?.template.source.kind === "bike" ? view.template.name : t("bike.placeholderTemplate")}
        </span>
      </div>
      {view?.template.problem && (
        <p className="flex items-center gap-1 text-[12px] text-amber-500">
          <AlertTriangle className="size-3.5" />
          {t("bike.templateProblem", { problem: view.template.problem })}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <BikePicker onPick={onPickTemplate} disabled={busy !== null} />
        <Button size="sm" variant="ghost" onClick={onChooseTemplate} disabled={busy !== null}>
          <FolderOpen className="size-3.5" />
          {t("bike.chooseTemplate")}
        </Button>
        {view?.template.source.kind === "bike" && (
          <Button size="sm" variant="ghost" onClick={() => onPickTemplate(null)} disabled={busy !== null}>
            {t("bike.usePlaceholder")}
          </Button>
        )}
      </div>

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
              <span className="text-muted-foreground">{t("bike.nudgeFor", { role: t(`bike.role.${sel.role}`) })}</span>
              {moves.map(({ icon: Icon, label, d }) => (
                <Button key={label} size="sm" variant="outline" onClick={() => onNudge(d)} title={t(label)}>
                  <Icon className="size-3.5" />
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

      <div className="mt-auto flex items-center gap-2 border-t border-border pt-3">
        <Button size="sm" variant="ghost" onClick={onPlaceholder} disabled={busy !== null} title={t("bike.placeholderHint")}>
          {busy === "placeholder" ? <Loader2 className="size-3.5 animate-spin" /> : <Bike className="size-3.5" />}
          {t("bike.addPlaceholder")}
        </Button>
      </div>
    </section>
  );
}

/** Loads the assembly once and on every `version` bump, for `BikeBuilder` to hand down. */
export function useAssemblyView(version: number) {
  const t = useT();
  const [view, setView] = useState<AssemblyView | null>(null);
  const load = useCallback(() => {
    getAssembly()
      .then(setView)
      .catch((e) => toast.error(t("bike.assemblyFailed"), { description: String(e) }));
  }, [t]);
  useEffect(() => load(), [load, version]);
  return [view, setView] as const;
}
