import { useCallback, useEffect, useMemo, useState } from "react";
import { Grid2x2, Keyboard, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useNavStyle } from "@frost/shared/lib/viewportNav";
import { useT } from "@/i18n";
import { MOUNT_ROLES, getAssembly, type AssemblyView, type LibraryPart, type Role, type V3 } from "../../../api/bikebuild";
import type { BikeCommand } from "./bikeKeys";
import Preview3D from "./Preview3D";
import type { BuildStep } from "./StepHeader";
import { Kbd, Tip } from "./Tip";
import type { useBikeLibrary } from "./useBikeLibrary";

/**
 * The middle of the tab: the 3D view, and only that. Picking a part, moving it and turning
 * it all moved to the outliner and the Properties panel either side; what stays here is what
 * belongs to the view itself — click a part to select it, click an open mount dot to fill
 * it, drop a part from the outliner onto it, or use the Blender keys with the pointer over it
 * (the `?` in its corner lists them). Parts hidden with the outliner's eye (or H) aren't
 * drawn (a view setting: the build still has them).
 *
 * Before there's a base bike it says that's the first step, with the button that takes it;
 * after, one line along the top says what comes next.
 */
export default function PreviewPane({
  version,
  view,
  lib,
  baseChosen,
  reached,
  selectedRoles,
  transformRole,
  onSelectRole,
  hidden,
  onCommand,
  onGrab,
  onRotate,
  onPickBase,
  takingOff,
  onTakeOff,
  onCancelTakeOff,
  helpOpen,
  onHelpOpen,
  flash,
  onNothingToTransform,
  onFill,
}: {
  version: number;
  view: AssemblyView | null;
  lib: ReturnType<typeof useBikeLibrary>;
  /** Nothing's been picked for the base yet — mount dots floating over an empty grid don't
   *  mean anything until there's a bike they belong to, so they're hidden. */
  baseChosen: boolean;
  /** How far the build has got: which next-step line shows. */
  reached: BuildStep;
  selectedRoles: Role[];
  transformRole: Role | null;
  onSelectRole: (role: Role | null) => void;
  /** Part ids the outliner's eye (or H) has hidden. */
  hidden: Set<string>;
  onCommand: (cmd: BikeCommand) => void;
  onGrab: (role: Role, delta: V3) => void;
  onRotate: (role: Role, rotation: V3) => void;
  onPickBase: () => void;
  /** X was pressed on this part: asking before it comes off. */
  takingOff: LibraryPart | null;
  onTakeOff: (part: LibraryPart) => void;
  onCancelTakeOff: () => void;
  helpOpen: boolean;
  onHelpOpen: (open: boolean) => void;
  /** A short note over the view (a key that had nothing to act on), or null. */
  flash: string | null;
  onNothingToTransform: () => void;
  /** A part picked for an open mount: into `role`'s slot. */
  onFill: (role: Role, id: string) => void;
}) {
  const t = useT();
  /** An open mount was clicked: which roles it takes, so a quick-pick panel can offer parts
   *  of one of them right there. */
  const [mountPick, setMountPick] = useState<Role[] | null>(null);

  function onMountClick(mounts: string[]) {
    // Every role any of the clicked mounts takes, deduplicated — more than one mount can
    // share a point (see `Preview3D`'s note on `steer_axis`/`fork_clamp`).
    const roles = [...new Set(mounts.flatMap((m) => MOUNT_ROLES[m] ?? []))];
    if (roles.length) setMountPick(roles);
  }

  const all = view?.assembly.placed;
  const placed = useMemo(() => (all ?? []).filter((p) => !hidden.has(p.partId)), [all, hidden]);
  const hint = reached === 2 ? t("bike.hint.step2") : reached === 3 ? t("bike.hint.step3") : null;

  return (
    // The marker `usePartDrag` looks for on drop.
    <section data-bike-viewport className="relative h-full min-h-0 min-w-0">
      <Preview3D
        placed={placed}
        anchors={baseChosen ? (view?.assembly.anchors ?? {}) : {}}
        selected={selectedRoles}
        onSelect={onSelectRole}
        onMountClick={onMountClick}
        version={version}
        transformRole={transformRole}
        onCommand={onCommand}
        onGrab={onGrab}
        onRotate={onRotate}
        onNothingToTransform={onNothingToTransform}
      />
      {!baseChosen ? (
        <FirstRun onPickBase={onPickBase} />
      ) : (
        hint && (
          <p
            data-bike-hint
            className="pointer-events-none absolute left-3 right-24 top-2.5 text-[12px] leading-snug text-muted-foreground [text-shadow:0_1px_2px_rgb(0_0_0/0.6)]"
          >
            {hint}
          </p>
        )
      )}
      {flash && (
        <p
          role="status"
          className="pointer-events-none absolute left-1/2 top-10 z-20 -translate-x-1/2 rounded border border-border bg-popover/95 px-3 py-1.5 text-[12px] shadow-lg"
        >
          {flash}
        </p>
      )}
      {takingOff && <TakeOffConfirm part={takingOff} onYes={() => onTakeOff(takingOff)} onNo={onCancelTakeOff} />}
      <BlenderKeysButton open={helpOpen} onOpen={onHelpOpen} />
      {mountPick && <MountPickPopover roles={mountPick} lib={lib} onFill={onFill} onClose={() => setMountPick(null)} />}
    </section>
  );
}

/** Before anything else: the one thing to do first, and the button that does it. */
function FirstRun({ onPickBase }: { onPickBase: () => void }) {
  const t = useT();
  return (
    <div data-bike-first-run className="absolute inset-0 flex items-center justify-center p-6">
      <div className="flex max-w-sm flex-col items-center gap-3 border border-border bg-popover/90 px-6 py-5 text-center shadow-lg">
        <Grid2x2 className="size-6 text-primary" />
        <p className="text-[15px] font-semibold">{t("bike.firstRunTitle")}</p>
        <p className="text-[12.5px] leading-relaxed text-muted-foreground">{t("bike.firstRunBody")}</p>
        <Button size="sm" onClick={onPickBase}>
          <Grid2x2 className="size-3.5" />
          {t("bike.pickBaseBike")}
        </Button>
      </div>
    </div>
  );
}

/** X / Delete asks first, in the view, as Blender's own "Delete?" does. Enter or the button
 *  takes it off; Esc or Cancel doesn't. */
function TakeOffConfirm({ part, onYes, onNo }: { part: LibraryPart; onYes: () => void; onNo: () => void }) {
  const t = useT();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code === "Enter" || e.code === "NumpadEnter" || e.code === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (e.code === "Escape") onNo();
        else onYes();
      }
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [onYes, onNo]);
  return (
    <div
      role="alertdialog"
      aria-label={t("bike.takeOff")}
      className="absolute left-1/2 top-10 z-30 flex w-max max-w-[calc(100%-2rem)] -translate-x-1/2 items-center gap-2 border border-border bg-popover px-3 py-2 text-[12.5px] shadow-lg"
    >
      <span className="min-w-0">{t("bike.takeOffConfirm", { name: part.name })}</span>
      <Button size="sm" variant="destructive" className="h-6 px-2 text-[11.5px]" onClick={onYes} autoFocus>
        {t("bike.takeOff")}
        <Kbd className="border-white/30 bg-transparent text-current">Enter</Kbd>
      </Button>
      <Button size="sm" variant="ghost" className="h-6 px-2 text-[11.5px]" onClick={onNo}>
        {t("common.cancel")}
        <Kbd>Esc</Kbd>
      </Button>
    </div>
  );
}

/** The `?` in the view's corner: the Blender keys, listed. `?` over the view opens it too. */
function BlenderKeysButton({ open, onOpen }: { open: boolean; onOpen: (open: boolean) => void }) {
  const t = useT();
  const [style] = useNavStyle();
  const rows: [string, string][] = [
    ["G", t("bike.keys.g")],
    ["R", t("bike.keys.r")],
    ["Enter · Esc", t("bike.keys.confirm")],
    ["H · Alt+H", t("bike.keys.h")],
    ["X · Delete", t("bike.keys.x")],
    ["A · Alt+A", t("bike.keys.a")],
    ["N", t("bike.keys.n")],
    ["Ctrl+Z · Ctrl+Shift+Z", t("bike.keys.undo")],
    ["Shift+A", t("bike.keys.add")],
    ["P · Ctrl+P", t("bike.keys.split")],
    ["Numpad 1 3 7 · . · Home · `", t("bike.keys.view")],
    [style === "blender" ? t("bike.keys.mouseBlender") : t("bike.keys.mouseClassic"), t("bike.keys.mouse")],
  ];
  return (
    <>
      <Tip label={t("bike.keys.title")} keys="?" side="top" className="absolute bottom-2 left-2 z-20">
        <button
          type="button"
          aria-label={t("bike.keys.title")}
          aria-expanded={open}
          onClick={() => onOpen(!open)}
          className="flex size-7 items-center justify-center rounded-full border border-border bg-popover/90 text-[13px] font-semibold text-muted-foreground shadow hover:text-foreground"
        >
          ?
        </button>
      </Tip>
      {open && (
        <div
          role="dialog"
          aria-label={t("bike.keys.title")}
          data-bike-keys-help
          className="absolute bottom-11 left-2 z-30 flex max-h-[calc(100%-4rem)] w-[25rem] max-w-[calc(100%-1rem)] flex-col gap-2 overflow-y-auto border border-border bg-popover/95 p-3 text-[12px] shadow-xl backdrop-blur"
        >
          <div className="flex items-center gap-2">
            <Keyboard className="size-3.5 text-primary" />
            <span className="font-semibold">{t("bike.keys.title")}</span>
            <button type="button" onClick={() => onOpen(false)} className="ml-auto text-faint hover:text-foreground" aria-label={t("common.close")}>
              <X className="size-3.5" />
            </button>
          </div>
          <dl className="grid grid-cols-[8.5rem_minmax(0,1fr)] gap-x-3 gap-y-1.5">
            {rows.map(([keys, what]) => (
              <div key={keys} className="contents">
                <dt className="flex flex-wrap items-start gap-1">
                  {keys.split(" · ").map((k) => (
                    <Kbd key={k}>{k}</Kbd>
                  ))}
                </dt>
                <dd className="text-muted-foreground">{what}</dd>
              </div>
            ))}
          </dl>
          <p className="text-[11px] text-faint">{t("bike.keys.hover")}</p>
        </div>
      )}
    </>
  );
}

/** The panel a mount click opens: parts of the role(s) that mount takes, to fill it without
 *  going to find the slot in the outliner. Floats over the bottom of the view. */
function MountPickPopover({
  roles,
  lib,
  onFill,
  onClose,
}: {
  roles: Role[];
  lib: ReturnType<typeof useBikeLibrary>;
  onFill: (role: Role, id: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const candidates = (lib.parts ?? []).filter((p) => p.role && roles.includes(p.role));

  return (
    <div className="absolute inset-x-12 bottom-3 z-20 flex max-h-40 flex-col gap-1 overflow-y-auto border border-border bg-popover/95 p-2 shadow-lg backdrop-blur">
      <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-faint">
        {roles.map((r) => t(`bike.role.${r}`)).join(" / ")}
        <button type="button" onClick={onClose} className="ml-auto text-faint hover:text-foreground">
          <X className="size-3.5" />
        </button>
      </div>
      {candidates.length === 0 ? (
        <p className="px-1 text-[12px] text-muted-foreground">{t("bike.noPartsForRoleHint")}</p>
      ) : (
        candidates.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => {
              if (p.role) onFill(p.role, p.id);
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
