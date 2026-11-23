import { useEffect, useState } from "react";
import { AlertTriangle, ArrowRightToLine, Box, Plus, RotateCcw, Undo2 } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { cn } from "@frost/shared/lib/utils";
import { getLocale, useT } from "@/i18n";
import {
  ROLES,
  type AssemblyView,
  type LibraryPart,
  type Placed,
  type Role,
  type V3,
} from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";
import type { PartActions, Selection } from "./Outliner";

/** Where a part sits and how it's turned, set the way G and R set them: through
 *  `BikeBuilder`, which keeps each change for undo. Absolute: the nudge in metres and the
 *  turn in degrees it should end up at, null to put it back as it snapped. */
export interface TransformEdits {
  setNudge: (role: Role, nudge: V3 | null) => void;
  setRotation: (role: Role, rotation: V3 | null) => void;
}

/** "LOD1, LOD2 and shadow", in the app's language. */
function lodList(levels: string[], shadow: string): string {
  const names = levels.map((l) => (l === "SHADOW" ? shadow : l));
  // Intl.ListFormat is in every WebView2 Studio runs on, just not in this tsconfig's lib.
  const ListFormat = (
    Intl as unknown as {
      ListFormat?: new (locale: string, opts: { type: string }) => { format(items: string[]): string };
    }
  ).ListFormat;
  try {
    if (ListFormat) return new ListFormat(getLocale(), { type: "conjunction" }).format(names);
  } catch {
    // An odd locale tag: fall through to the plain list.
  }
  return names.join(", ");
}

/** MX Bikes' own convention for a shadow-only mesh (see `bikebuild.rs`): by the part's name
 *  or its file's name, never the folder it's in. */
function isShadow(p: LibraryPart): boolean {
  const fileName = p.source.split(/[\\/]/).pop() ?? p.source;
  return /shadow/i.test(p.name) || /shadow/i.test(fileName);
}

/**
 * Properties, Blender-style: what's selected in the outliner (or clicked in the 3D view),
 * and the settings that belong to it — its slot, where it sits and how it's turned, its
 * triangles and LODs, and anything wrong with it on one compact line.
 *
 * Location is the part's move from where it snapped, in millimetres; rotation its turn about
 * its joint, in degrees — both in Blender's axes, as the build writes them. An empty slot
 * selected shows the parts that could fill it.
 */
export default function Properties({
  selection,
  lib,
  view,
  edits,
  actions,
  onAdd,
  addSlot,
}: {
  selection: Selection;
  lib: ReturnType<typeof useBikeLibrary>;
  view: AssemblyView | null;
  edits: TransformEdits;
  actions: PartActions;
  /** "Add part", for an empty slot with nothing to fill it. Goes through the Blender gate. */
  onAdd: () => void;
  /** Where the "needs Blender" line for that add shows, when it does. */
  addSlot?: React.ReactNode;
}) {
  const t = useT();
  const part = selection?.kind === "part" ? lib.parts?.find((p) => p.id === selection.id) ?? null : null;

  return (
    <section
      data-dock="right"
      aria-label={t("bike.properties")}
      className="flex h-full min-h-0 min-w-0 flex-col bg-window/40"
    >
      <header className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.properties")}</h2>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-3 text-[12.5px]">
        {part ? (
          <PartProps part={part} lib={lib} view={view} edits={edits} actions={actions} />
        ) : selection?.kind === "slot" ? (
          <SlotProps role={selection.role} lib={lib} actions={actions} onAdd={onAdd} addSlot={addSlot} />
        ) : selection?.kind === "all" ? (
          <div className="flex flex-col gap-1">
            <p className="font-medium">{t("bike.allSelected", { n: view?.assembly.placed.length ?? 0 })}</p>
            <p className="text-[11.5px] text-muted-foreground">{t("bike.allSelectedHint")}</p>
          </div>
        ) : (
          <p className="text-muted-foreground">{t("bike.nothingSelected")}</p>
        )}
      </div>
    </section>
  );
}

function Label({ children }: { children: React.ReactNode }) {
  return <h3 className="mb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-faint">{children}</h3>;
}

function PartProps({
  part,
  lib,
  view,
  edits,
  actions,
}: {
  part: LibraryPart;
  lib: ReturnType<typeof useBikeLibrary>;
  view: AssemblyView | null;
  edits: TransformEdits;
  actions: PartActions;
}) {
  const t = useT();
  const placed = !!part.role && lib.slots[part.role] === part.id;
  const onBike: Placed | null = placed ? (view?.assembly.placed.find((p) => p.role === part.role) ?? null) : null;

  const warnings = [
    part.missing && t("bike.partMissing"),
    part.stale && t("bike.partStale"),
    part.missingTextures.length > 0 && t("bike.missingTextures", { names: part.missingTextures.join(", ") }),
    part.multiPartHint && t("bike.wholeBikeWarning"),
  ].filter((w): w is string => !!w);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex min-w-0 items-center gap-2.5">
        {part.thumb ? (
          <img src={part.thumb} alt="" draggable={false} className="size-11 shrink-0 bg-muted/40 object-contain" />
        ) : (
          <div className="flex size-11 shrink-0 items-center justify-center bg-muted/40 text-faint">
            <Box className="size-4" />
          </div>
        )}
        <div className="min-w-0">
          <p className="truncate font-medium" title={part.name}>
            {part.name}
          </p>
          <p className="truncate font-mono text-[10.5px] text-faint" title={part.source}>
            {part.source.split(/[\\/]/).pop()}
          </p>
        </div>
      </div>

      {/* Each in full, never cut short: the end of a warning is what to do about it. */}
      {warnings.map((w) => (
        <p key={w} className="flex min-w-0 items-start gap-1.5 text-[11.5px] leading-snug text-amber-500">
          <AlertTriangle className="mt-px size-3.5 shrink-0" />
          <span className="min-w-0 [overflow-wrap:anywhere]">{w}</span>
        </p>
      ))}

      <div>
        <Label>{t("bike.slot")}</Label>
        <Select
          value={part.role ?? NONE}
          onValueChange={(v) => void lib.onMoveToSlot(part, v, placed)}
          disabled={lib.busy}
        >
          <SelectTrigger className="h-8">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NONE}>{t("bike.noRole")}</SelectItem>
            {ROLES.map((r) => (
              <SelectItem key={r} value={r}>
                {t(`bike.role.${r}`)}
                {part.role === r && part.roleGuessed ? ` · ${t("bike.guessed")}` : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="mt-1.5 flex items-center gap-2">
          <span className={cn("text-[11.5px]", placed ? "text-success" : "text-faint")}>
            {placed ? t("bike.onTheBike") : t("bike.notPlaced")}
          </span>
          {placed ? (
            <Button
              size="sm"
              variant="ghost"
              className="ml-auto h-6 px-2 text-[11.5px]"
              onClick={() => actions.takeOff(part)}
              disabled={lib.busy}
            >
              <Undo2 className="size-3" />
              {t("bike.takeOff")}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="secondary"
              className="ml-auto h-6 px-2 text-[11.5px]"
              onClick={() => actions.place(part)}
              disabled={lib.busy || !part.role}
              title={!part.role ? t("bike.placeNeedsRole") : undefined}
            >
              <ArrowRightToLine className="size-3" />
              {t("bike.place")}
            </Button>
          )}
        </div>
      </div>

      {onBike && <Transform placed={onBike} edits={edits} />}

      <div>
        <Label>{t("bike.parts")}</Label>
        <p className="text-muted-foreground">
          {t("bike.partStats", { tris: part.tris.toLocaleString(), empties: part.empties.length })}
        </p>
        {part.lod && (
          <p className="mt-0.5 text-[11.5px] text-muted-foreground">
            {t("bike.lodUsed", { tris: part.lod.tris.toLocaleString(), levels: lodList(part.lod.leftOut, t("bike.lodShadow")) })}
          </p>
        )}
        {isShadow(part) && <p className="mt-0.5 text-[11.5px] text-muted-foreground">{t("bike.shadowModel")}</p>}
      </div>
    </div>
  );
}

const AXES = [
  { axis: "X", tip: "bike.axisX" },
  { axis: "Y", tip: "bike.axisY" },
  { axis: "Z", tip: "bike.axisZ" },
] as const;

/** Location and rotation of a part on the bike, as three fields each: the same values G
 *  and R write. */
function Transform({ placed, edits }: { placed: Placed; edits: TransformEdits }) {
  const t = useT();
  const mm = placed.nudge.map((v) => Math.round(v * 10_000) / 10) as V3;
  const deg = (placed.rotation ?? [0, 0, 0]) as V3;

  function setLocation(i: number, value: number) {
    const target = [...placed.nudge] as V3;
    target[i] = Math.max(-500, Math.min(500, value)) / 1000;
    if (Math.abs(target[i] - placed.nudge[i]) < 1e-5) return;
    edits.setNudge(placed.role, target);
  }

  const setRotation = (next: V3 | null) => edits.setRotation(placed.role, next);
  const resetLocation = () => edits.setNudge(placed.role, null);

  const moved = placed.nudge.some((v) => v !== 0);
  const turned = deg.some((v) => v !== 0);

  return (
    <>
      <div>
        <div className="flex items-center">
          <Label>{t("bike.location")}</Label>
          <ResetButton show={moved} title={t("bike.resetNudge")} onClick={resetLocation} />
        </div>
        <div className="grid grid-cols-3 gap-1">
          {AXES.map(({ axis, tip }, i) => (
            <NumField
              key={axis}
              axis={axis}
              title={t(tip)}
              value={mm[i]}
              step={1}
              min={-500}
              max={500}
              onCommit={(v) => void setLocation(i, v)}
            />
          ))}
        </div>
        <p className="mt-1 text-[10.5px] text-faint">
          {t("bike.snappedBy", { by: t(`bike.by.${placed.by === "as modelled" ? "modelled" : placed.by}`) })}
          {" · "}
          {t("bike.keys.gHint")}
        </p>
      </div>
      <div>
        <div className="flex items-center">
          <Label>{t("bike.rotation")}</Label>
          <ResetButton show={turned} title={t("bike.resetRotation")} onClick={() => setRotation(null)} />
        </div>
        <div className="grid grid-cols-3 gap-1">
          {AXES.map(({ axis, tip }, i) => (
            <NumField
              key={axis}
              axis={axis}
              title={t(tip)}
              value={deg[i]}
              step={1}
              min={-180}
              max={180}
              onCommit={(v) => {
                const next = [...deg] as V3;
                next[i] = Math.max(-180, Math.min(180, v));
                setRotation(next);
              }}
            />
          ))}
        </div>
      </div>
    </>
  );
}

function ResetButton({ show, title, onClick }: { show: boolean; title: string; onClick: () => void }) {
  if (!show) return null;
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className="mb-1.5 ml-auto text-faint hover:text-foreground"
    >
      <RotateCcw className="size-3" />
    </button>
  );
}

/** One axis: Blender's look — the axis letter inside the field, the number to its right.
 *  Commits on Enter or leaving the field (Esc puts it back), and the arrow keys step it. */
function NumField({
  axis,
  title,
  value,
  step,
  min,
  max,
  onCommit,
}: {
  axis: "X" | "Y" | "Z";
  title: string;
  value: number;
  step: number;
  min: number;
  max: number;
  onCommit: (v: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const colour = axis === "X" ? "text-red-400" : axis === "Y" ? "text-green-400" : "text-blue-400";

  function commit() {
    const v = Number(draft.replace(",", "."));
    if (draft.trim() === "" || !Number.isFinite(v)) {
      setDraft(String(value));
      return;
    }
    if (v !== value) onCommit(v);
  }

  return (
    <label
      title={title}
      className="flex h-7 min-w-0 items-center gap-1 border border-input bg-background/60 px-1.5 focus-within:border-ring"
    >
      <span className={cn("text-[10.5px] font-semibold", colour)}>{axis}</span>
      <input
        type="number"
        inputMode="decimal"
        value={draft}
        step={step}
        min={min}
        max={max}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setDraft(String(value));
            (e.target as HTMLInputElement).blur();
          }
        }}
        className="w-full min-w-0 bg-transparent text-right font-mono text-[11.5px] tabular-nums outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
      />
    </label>
  );
}

function SlotProps({
  role,
  lib,
  actions,
  onAdd,
  addSlot,
}: {
  role: (typeof ROLES)[number];
  lib: ReturnType<typeof useBikeLibrary>;
  actions: PartActions;
  onAdd: () => void;
  addSlot?: React.ReactNode;
}) {
  const t = useT();
  const candidates = (lib.parts ?? []).filter((p) => p.role === role);
  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="font-medium">{t(`bike.role.${role}`)}</p>
        <p className="mt-0.5 text-[11.5px] text-muted-foreground">{t(`bike.fits.${role}`)}</p>
        <p className="mt-1.5 text-[11.5px] text-muted-foreground">{t("bike.emptySlotHint")}</p>
      </div>
      <div>
        <Label>{t("bike.pickAPart")}</Label>
        {candidates.length === 0 ? (
          <p className="text-[11.5px] text-faint">{t("bike.noPartsForRole")}</p>
        ) : (
          <ul className="flex flex-col">
            {candidates.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  disabled={lib.busy}
                  onClick={() => actions.place(p)}
                  title={p.source}
                  className="flex w-full items-center gap-2 px-1.5 py-1 text-left hover:bg-accent disabled:opacity-50"
                >
                  <ArrowRightToLine className="size-3.5 shrink-0 text-faint" />
                  <span className="min-w-0 flex-1 truncate">{p.name}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <Button size="sm" variant="outline" className="self-start" onClick={onAdd} disabled={lib.busy}>
        <Plus className="size-3.5" />
        {t("bike.addPart")}
      </Button>
      {addSlot}
    </div>
  );
}
