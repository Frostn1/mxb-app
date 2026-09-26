import { useCallback, useEffect, useState } from "react";
import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ChevronsDown,
  ChevronsUp,
  RotateCcw,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import {
  MOUNT_ROLES,
  getAssembly,
  nudge,
  type AssemblyView,
  type Role,
  type V3,
} from "../../../api/bikebuild";
import Preview3D from "./Preview3D";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";

/** Nudge steps, metres. */
const STEPS = [0.001, 0.005, 0.01];

/**
 * The centre panel: the bike in 3D, first. Sean's ask, plainly — "have a part as the whole
 * bike, see it in 3D, and place on it" — so the viewport is the dominant thing here, mounted
 * whether or not anything is placed yet (a base bike or an installed template shows on its
 * own), and it's how a part gets placed: drag one from the tray onto it, or click an open
 * mount dot to pick a part for the role that mount takes. Nudging the selected part is the
 * only other thing this panel does — choosing the base bike moved up to `BaseBikeStep`, which
 * is a decision made once per build, not something that belongs beside the thing it decides.
 */
export default function PreviewPane({
  version,
  view,
  setView,
  lib,
}: {
  version: number;
  view: AssemblyView | null;
  setView: (v: AssemblyView) => void;
  lib: ReturnType<typeof useBikeLibrary>;
}) {
  const t = useT();
  const [selected, setSelected] = useState<Role | null>(null);
  const [step, setStep] = useState(STEPS[1]);
  /** An open mount was clicked: which roles it takes, so a quick-pick panel can offer parts
   *  of one of them instead of the rider going to find the right slot in the outliner. */
  const [mountPick, setMountPick] = useState<Role[] | null>(null);

  async function onNudge(delta: V3 | null) {
    if (!selected) return;
    try {
      setView(await nudge(selected, delta));
    } catch (e) {
      toast.error(t("bike.nudgeFailed"), { description: String(e) });
    }
  }

  function onMountClick(mounts: string[]) {
    // Every role any of the clicked mounts takes, deduplicated — more than one mount can
    // share a point (see `Preview3D`'s note on `steer_axis`/`fork_clamp`).
    const roles = [...new Set(mounts.flatMap((m) => MOUNT_ROLES[m] ?? []))];
    if (roles.length) setMountPick(roles);
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
    <section className="flex h-full min-w-0 flex-col gap-2 p-4">
      {/* The marker `usePartDrag` looks for on drop — the pointer-based drag from the tray
          checks `elementFromPoint` against this rather than a raycast onto a particular
          mount, since a part's role already says exactly where it goes. */}
      <div data-bike-viewport className="relative min-h-0 flex-1">
        <Preview3D
          placed={placed}
          anchors={view?.assembly.anchors ?? {}}
          selected={selected}
          onSelect={setSelected}
          onMountClick={onMountClick}
          version={version}
        />
        {placed.length === 0 && (
          <p className="pointer-events-none absolute inset-x-4 top-3 text-[12px] text-muted-foreground">
            {t("bike.assembleEmpty")}
          </p>
        )}
        {mountPick && (
          <MountPickPopover
            roles={mountPick}
            lib={lib}
            onClose={() => setMountPick(null)}
          />
        )}
      </div>

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
          <Button
            size="sm"
            variant="ghost"
            onClick={() => void lib.onSlot(sel.role, NONE)}
            title={t("bike.removeFromSlot")}
          >
            <X className="size-3.5" />
          </Button>
          <span className="font-mono text-[11px] text-muted-foreground">
            {t("bike.nudged", { x: mm(sel.nudge[0]), y: mm(sel.nudge[1]), z: mm(sel.nudge[2]) })} ·{" "}
            {t("bike.snappedBy", { by: t(`bike.by.${sel.by === "as modelled" ? "modelled" : sel.by}`) })}
          </span>
        </div>
      ) : (
        <p className="text-[12px] text-muted-foreground">{t("bike.pickToNudge")}</p>
      )}
    </section>
  );
}

/** The panel a mount click opens: parts of the role(s) that mount takes, to fill it without
 *  going to find the slot elsewhere. Floats over the bottom of the viewport rather than
 *  anchoring to the clicked point in 3D, which would need projecting the click back to screen
 *  space for a payoff — a fixed spot a rider already glances at reads just as clearly. */
function MountPickPopover({
  roles,
  lib,
  onClose,
}: {
  roles: Role[];
  lib: ReturnType<typeof useBikeLibrary>;
  onClose: () => void;
}) {
  const t = useT();
  const candidates = (lib.parts ?? []).filter((p) => p.role && roles.includes(p.role));

  return (
    <div className="absolute inset-x-3 bottom-3 flex max-h-40 flex-col gap-1 overflow-y-auto border border-border bg-popover/95 p-2 shadow-lg backdrop-blur">
      <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-faint">
        {roles.map((r) => t(`bike.role.${r}`)).join(" / ")}
        <button type="button" onClick={onClose} className="ml-auto text-faint hover:text-foreground">
          <X className="size-3.5" />
        </button>
      </div>
      {candidates.length === 0 ? (
        <p className="px-1 text-[12px] text-muted-foreground">{t("bike.noPartsForRole")}</p>
      ) : (
        candidates.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => {
              if (p.role) void lib.onSlot(p.role, p.id);
              onClose();
            }}
            className="flex items-center gap-2 px-1 py-1 text-left text-[12px] hover:bg-accent"
          >
            <span className="min-w-0 flex-1 truncate">{p.name}</span>
          </button>
        ))
      )}
    </div>
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
