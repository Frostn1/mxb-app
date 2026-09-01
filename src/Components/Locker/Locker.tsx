import { useCallback, useEffect, useState } from "react";
import {
  Bike,
  Volume2,
  Check,
  RefreshCw,
  Loader2,
  AlertTriangle,
  Ban,
  Box,
  Palette,
  FolderInput,
  Link2,
  Link2Off,
  Wrench,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { Share2 } from "lucide-react";
import { cn } from "@/lib/utils";
import HelpHint from "@/Components/ui/help-hint";
import {
  scanModelSwaps,
  applyModelSwap,
  detectLooseSwaps,
  detectOrphanedSetup,
  repairOrphanedSetup,
  onModsChanged,
  scanSoundSwaps,
  applySoundSwap,
  bindSound,
  unbindSound,
} from "../../api/mods";
import type {
  BikeModels,
  BikeSounds,
  LooseSwapBike,
  ModelVariant,
  OrphanedSetup,
  SoundVariant,
  SwapApplyOutcome,
} from "../../types";
import RegisterSwapsDialog from "./RegisterSwapsDialog";
import AssignPaintsDialog from "./AssignPaintsDialog";
import { ModelSwapActions } from "./ModelSwapActions";
import { ViewerDialog } from "../Viewer/ViewerDialog";
import { useConfig } from "../../Context/Config";
import { Trans } from "../../i18n";
import { useT, type TFunc } from "../../i18n/context";
import { useShare } from "../../Context/Share";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
} from "@/Components/ui/context-menu";

/**
 * Locker — the app-side bike **model & sound swap** manager, twinned with FrostMod's
 * in-game swappers. For each *extracted* bike it lists the model sets (under
 * `<Bike>/FrostMod Models/`) and sound sets (under `<Bike>/FrostMod Sounds/`) you
 * have, marks the active one of each, and lets you switch — the same backup-current
 * / move-in file dance FrostMod does, so the two stay interchangeable. Packed `.pkz`
 * bikes have no swappable model and only appear if they carry sound files.
 *
 * Model and sound live loose at the same bike root but swap independently, so
 * switching a model preserves the sound (and vice versa). A sound can optionally be
 * **bound** to a model swap, so activating that model pulls its sound along.
 */

/**
 * Orphan warnings the user has hidden, keyed on the bike *and* the files it's missing —
 * a different breakage on the same bike is news again. Stored per machine, since which
 * bikes are broken is a property of the install, not the account.
 */
const HIDDEN_ORPHANS_KEY = "mxb:orphanWarningsHidden:v1";

function orphanKey(o: OrphanedSetup): string {
  return `${o.bike}/${[...o.files].sort().join(",")}`;
}

function readHiddenOrphans(): Set<string> {
  return new Set(
    (localStorage.getItem(HIDDEN_ORPHANS_KEY) ?? "").split("|").filter(Boolean),
  );
}

function writeHiddenOrphans(keys: Set<string>): Set<string> {
  localStorage.setItem(HIDDEN_ORPHANS_KEY, [...keys].join("|"));
  return keys;
}

/** Forget hidden warnings whose breakage is gone, so a repaired bike that breaks the
 *  same way later warns afresh instead of staying silently hidden forever. */
function pruneHiddenOrphans(live: OrphanedSetup[]): Set<string> {
  const alive = new Set(live.map(orphanKey));
  return writeHiddenOrphans(new Set([...readHiddenOrphans()].filter((k) => alive.has(k))));
}

/**
 * Trailing feedback for a swap toast.
 *
 * Models and sounds get different notes because they refresh by different routes.
 * `live_refresh` re-runs the game's *customization* loader — that reloads paints and
 * gear but never the bike mesh, so it says nothing about whether a swapped model is
 * visible. Nothing reloads the mesh: FrostMod v0.9.11 removed the live re-apply because
 * it crashed the game, so every model outcome ends in the same instruction — switch bike
 * category away and back, which is what actually re-reads the model. Reselecting the same
 * bike does not. `model_refresh` only decides who says it.
 */
function swapNote(
  kind: "model" | "sound",
  outcome: SwapApplyOutcome,
  t: TFunc,
): string {
  if (!outcome.game_running) return t("locker.loadsNextTime");

  if (kind === "model") {
    switch (outcome.model_refresh) {
      case "signaled":
      case "withheld":
        return t("locker.modelSwitchCategory");
      case "not_running":
        return t("locker.modelFrostmodNotRunning");
      case "write_failed":
        return t("locker.modelFrostmodUnreachable");
      case "unsupported":
        return t("locker.modelRefreshWindowsOnly");
      default: // null — instant refresh is switched off in Settings
        return t("locker.modelInstantRefreshOff");
    }
  }

  switch (outcome.live_refresh) {
    case "refreshed":
      return t("locker.refreshedLive");
    case "failed":
      return t("locker.refreshFailed");
    default:
      return t("locker.reselectProfile");
  }
}

/** Where each kind of swap set is filed inside its bike — `modelswap::LIB_DIR` and
 *  `soundmods::SOUND_LIB_DIR` on the Rust side. */
const SWAP_DIR = { model: "FrostMod Models", sound: "FrostMod Sounds" } as const;

/** A variant's path as the rest of the app names content: relative to the MX Bikes root. */
function variantRel(bike: string, kind: "model" | "sound", name: string): string {
  return `mods/bikes/${bike}/${SWAP_DIR[kind]}/${name}`;
}

/** The row standing for the game's own model/sound, which is never a folder in the library. */
function isStockRow(v: { name: string }): boolean {
  return v.name.toLowerCase() === "stock";
}

/** One bike's row: its models (null for sound-only bikes) and its sounds (always present). */
interface Row {
  bike: string;
  models: BikeModels | null;
  sounds: BikeSounds;
}

/** A Stock-only sounds view for a bike that has models but no sound library yet. */
function stockSounds(bike: string, activeModel: string): BikeSounds {
  return {
    bike,
    active: "Stock",
    activeModel,
    bindings: {},
    variants: [{ name: "Stock", active: true, valid: false, empty: true, fileCount: 0 }],
  };
}

/** Merge the two scans into one per-bike list (union of bike names), sorted by name. */
function mergeRows(models: BikeModels[], sounds: BikeSounds[]): Row[] {
  const soundByBike = new Map(sounds.map((s) => [s.bike, s]));
  const modelByBike = new Map(models.map((m) => [m.bike, m]));
  const names = new Set<string>([...modelByBike.keys(), ...soundByBike.keys()]);
  return [...names]
    .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()))
    .map((bike) => {
      const m = modelByBike.get(bike) ?? null;
      const s = soundByBike.get(bike) ?? stockSounds(bike, m?.active ?? "Original");
      return { bike, models: m, sounds: s };
    });
}

export default function Locker() {
  const t = useT();
  // Bike geometry needs the optional local module — without it there's nothing to show.
  const { bikePreview } = useConfig();
  // The swap being previewed in 3D, if any. Nothing on disk moves to show it.
  const [preview, setPreview] = useState<{ bike: string; variant: string } | null>(null);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bike name currently being mutated (disables its rows + spins the target).
  const [busy, setBusy] = useState<string | null>(null);
  // Model sets found sitting loose outside `FrostMod Models/` (banner + dialog).
  const [loose, setLoose] = useState<LooseSwapBike[]>([]);
  const [registerOpen, setRegisterOpen] = useState(false);
  // The model whose liveries are being assigned, with the bike's scan it was opened from.
  const [assigning, setAssigning] = useState<{ models: BikeModels; model: string } | null>(null);
  // Bikes gutted by a pre-0.6.3 swap — their setup files are in a swap folder.
  const [orphaned, setOrphaned] = useState<OrphanedSetup[]>([]);
  // Of those, the ones the user has hidden with the banner's ✕.
  const [hiddenOrphans, setHiddenOrphans] = useState<Set<string>>(readHiddenOrphans);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [models, sounds, detected, broken] = await Promise.all([
        scanModelSwaps(),
        scanSoundSwaps().catch(() => [] as BikeSounds[]),
        detectLooseSwaps().catch(() => [] as LooseSwapBike[]),
        detectOrphanedSetup().catch(() => null),
      ]);
      setRows(mergeRows(models, sounds));
      setLoose(detected);
      setOrphaned(broken ?? []);
      // Prune only off a scan that actually ran — a failed detection is no evidence the
      // breakage is fixed, and would drop the hidden warnings for nothing.
      if (broken) setHiddenOrphans(pruneHiddenOrphans(broken));
    } catch (e) {
      setError(String(e));
      setRows([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // Installing a mod or editing the folder changes what's swappable — pick it up without
  // making the user hit Rescan (same watcher `Context/Frostmod` listens to).
  useEffect(() => {
    const un = onModsChanged(() => void load());
    return () => {
      void un.then((f) => f());
    };
  }, [load]);

  const looseCount = loose.reduce((n, b) => n + b.candidates.length, 0);

  // Runs a mutation for `bike`, toasting success/failure and refreshing every scan.
  // `note` optionally appends live-refresh feedback derived from the result.
  const run = useCallback(
    async <T,>(
      bike: string,
      ok: string,
      fn: () => Promise<T>,
      note?: (result: T) => string,
    ) => {
      setBusy(bike);
      try {
        const result = await fn();
        toast.success(note ? `${ok} ${note(result)}` : ok);
        await load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(null);
      }
    },
    [load],
  );

  const onHideOrphan = useCallback((o: OrphanedSetup) => {
    setHiddenOrphans(writeHiddenOrphans(new Set(readHiddenOrphans()).add(orphanKey(o))));
  }, []);

  const onRepair = (bike: string) =>
    run(
      bike,
      t("locker.restored", { bike }),
      () => repairOrphanedSetup(bike),
      (n) => t("locker.restoredNote", { count: n }),
    );

  const onModelSwap = (bike: string, target: string) =>
    run(
      bike,
      t("locker.switchedModel", { bike, target }),
      () => applyModelSwap(bike, target),
      (r) => swapNote("model", r, t),
    );
  const onSoundSwap = (bike: string, target: string) =>
    run(
      bike,
      t("locker.switchedSound", { bike, target }),
      () => applySoundSwap(bike, target),
      (r) => swapNote("sound", r, t),
    );
  const onBind = (bike: string, model: string, sound: string) =>
    run(bike, t("locker.tied", { sound, model }), () => bindSound(bike, model, sound));
  const onUnbind = (bike: string, model: string, sound: string) =>
    run(bike, t("locker.untied", { sound, model }), () => unbindSound(bike, model));

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">
            {t("nav.locker")}
          </h1>
          <HelpHint title={t("nav.locker")} description={t("locker.help")} />
        </div>
        <button
          onClick={() => void load()}
          className="ml-auto flex items-center gap-1.5 rounded-lg border border-input bg-card px-3 py-2 text-[12.5px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <RefreshCw className={cn("size-3.5", rows === null && "animate-spin")} />
          {t("locker.rescan")}
        </button>
      </header>

      {orphaned
        .filter((o) => !hiddenOrphans.has(orphanKey(o)))
        .map((o) => (
          <div
            key={o.bike}
            className="mx-7 mb-3.5 flex items-center gap-2.5 rounded-lg border border-destructive/30 bg-destructive/[0.07] px-3.5 py-2.5"
          >
            <Wrench className="size-4 flex-none text-destructive/80" />
            <span className="min-w-0 flex-1 text-[12.5px] text-foreground/90">
              <Trans
                k="locker.orphanBanner"
                values={{
                  bike: <span className="font-semibold">{o.bike}</span>,
                  files: (
                    <span className="font-mono text-faint">{o.files.join(", ")}</span>
                  ),
                }}
              />
            </span>
            <button
              onClick={() => void onRepair(o.bike)}
              disabled={busy !== null}
              className="flex flex-none items-center gap-1.5 rounded-md bg-destructive/15 px-2.5 py-1.5 text-[12px] font-semibold text-destructive transition-colors hover:bg-destructive/25 disabled:opacity-50"
            >
              {busy === o.bike ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Wrench className="size-3.5" />
              )}
              {t("locker.restore")}
            </button>
            <button
              onClick={() => onHideOrphan(o)}
              aria-label={t("locker.hideOrphan")}
              title={t("locker.hideOrphan")}
              className="flex size-7 flex-none items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/15 hover:text-destructive"
            >
              <X className="size-3.5" />
            </button>
          </div>
        ))}

      {looseCount > 0 && (
        <button
          onClick={() => setRegisterOpen(true)}
          className="mx-7 mb-3.5 flex items-center gap-2.5 rounded-lg border border-amber-500/25 bg-amber-500/[0.07] px-3.5 py-2.5 text-left transition-colors hover:bg-amber-500/[0.12]"
        >
          <AlertTriangle className="size-4 flex-none text-amber-500/80" />
          <span className="min-w-0 flex-1 text-[12.5px] text-foreground/90">
            <Trans
              k="locker.looseBanner"
              count={looseCount}
              values={{
                modelsFolder: (
                  <span className="font-mono text-faint">FrostMod Models</span>
                ),
                soundsFolder: <span className="font-mono text-faint">Sounds</span>,
              }}
            />
          </span>
          <span className="flex flex-none items-center gap-1.5 text-[12px] font-semibold text-amber-500/90">
            <FolderInput className="size-3.5" />
            {t("locker.register")}
          </span>
        </button>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <p className="select-text py-16 text-center text-[13px] text-destructive">{error}</p>
        ) : rows === null ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {t("locker.scanning")}
          </p>
        ) : rows.length === 0 ? (
          <div className="mx-auto max-w-md py-14 text-[13px] text-muted-foreground">
            <p className="mb-3 text-center text-foreground/90">
              {t("locker.emptyTitle")}
            </p>
            <p className="mb-3">{t("locker.emptyIntro")}</p>
            <ol className="mb-4 list-decimal space-y-2 pl-5">
              <li>
                <Trans
                  k="locker.emptyRuleUnpacked"
                  values={{
                    unpacked: (
                      <span className="text-foreground/90">
                        {t("locker.unpacked")}
                      </span>
                    ),
                    path: (
                      <span className="mx-1 font-mono text-faint">
                        mods/bikes/&lt;Bike&gt;/
                      </span>
                    ),
                    pkz: <span className="font-mono text-faint">.pkz</span>,
                  }}
                />
              </li>
              <li>
                <Trans
                  k="locker.emptyRuleMesh"
                  values={{
                    edf: <span className="font-mono text-faint">.edf</span>,
                    folder: (
                      <span className="mx-1 font-mono text-faint">
                        FrostMod Models/
                      </span>
                    ),
                  }}
                />
              </li>
            </ol>
            <button
              onClick={() => void load()}
              className="mx-auto flex items-center gap-1.5 rounded-lg border border-input bg-card px-3 py-2 text-[12.5px] transition-colors hover:text-foreground"
            >
              <RefreshCw className="size-3.5" />
              {t("locker.scanForSwaps")}
            </button>
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            {rows.map((r) => (
              <BikeCard
                key={r.bike}
                row={r}
                busy={busy === r.bike}
                disabled={busy !== null}
                onModelSwap={onModelSwap}
                onSoundSwap={onSoundSwap}
                onAssignPaints={(models, model) => setAssigning({ models, model })}
                onPreview={
                  bikePreview
                    ? (bike, variant) => setPreview({ bike, variant })
                    : undefined
                }
                onBind={onBind}
                onUnbind={onUnbind}
                onChanged={() => void load()}
              />
            ))}
          </div>
        )}
      </div>

      <RegisterSwapsDialog
        open={registerOpen}
        onOpenChange={setRegisterOpen}
        bikes={loose}
        onDone={() => void load()}
      />

      {/* Keyed by bike + model so opening it for another model remounts: the tick state
          is seeded once, and reseeding mid-edit when a rescan lands would lose the edit. */}
      {assigning && (
        <AssignPaintsDialog
          key={`${assigning.models.bike}/${assigning.model}`}
          open
          onOpenChange={(o) => !o && setAssigning(null)}
          bike={assigning.models.bike}
          model={assigning.model}
          models={assigning.models}
          onDone={() => void load()}
        />
      )}

      <ViewerDialog
        open={preview !== null}
        onOpenChange={(o) => !o && setPreview(null)}
        title={preview ? `${preview.bike} · ${preview.variant}` : undefined}
        initialMode="bike"
        modelSwap={preview ?? undefined}
      />
    </div>
  );
}

function BikeCard({
  row,
  busy,
  disabled,
  onModelSwap,
  onSoundSwap,
  onPreview,
  onAssignPaints,
  onBind,
  onUnbind,
  onChanged,
}: {
  row: Row;
  busy: boolean;
  disabled: boolean;
  onModelSwap: (bike: string, target: string) => void;
  onSoundSwap: (bike: string, target: string) => void;
  /** Undefined when this build can't draw bike geometry — then no row offers a preview. */
  onPreview?: (bike: string, variant: string) => void;
  onAssignPaints: (models: BikeModels, model: string) => void;
  onBind: (bike: string, model: string, sound: string) => void;
  onUnbind: (bike: string, model: string, sound: string) => void;
  /** A model set moved or went to the Trash — rescan. */
  onChanged: () => void;
}) {
  const t = useT();
  const { bike, models, sounds } = row;
  const modelCount = models?.variants.length ?? 0;
  const soundCount = sounds.variants.length;

  return (
    <section className="flex flex-col gap-3.5 rounded-xl border border-white/[0.07] bg-card p-4">
      <div className="flex items-center gap-2.5">
        <div className="grid size-8 flex-none place-items-center rounded-md bg-foreground/[0.06] text-foreground/40">
          <Bike className="size-4" strokeWidth={1.5} />
        </div>
        <div className="min-w-0">
          <div className="truncate text-[14px] font-semibold">{bike}</div>
          <div className="truncate text-[11px] text-muted-foreground">
            {t("locker.summary", {
              model: models
                ? t("locker.modelNamed", { name: models.active })
                : t("locker.noModelSwaps"),
              sound: sounds.active,
            })}
          </div>
        </div>
      </div>

      {models && (
        <SwapSection
          icon={<Bike className="size-3.5" strokeWidth={1.75} />}
          label={t("locker.models")}
          hint={modelCount <= 1 ? t("locker.onlyOneModel") : undefined}
        >
          {models.variants.map((v) => (
            <VariantButton
              key={v.name}
              bike={bike}
              variant={v}
              kind="model"
              busy={busy}
              disabled={disabled}
              onClick={() => onModelSwap(bike, v.name)}
              // A set with a mesh can be drawn; so can Stock, which shows the packed model
              // the loose files are covering. A "no model" set has nothing to show.
              onPreview={
                onPreview && (v.valid || isStockRow(v))
                  ? () => onPreview(bike, v.name)
                  : undefined
              }
              manage={{ bike, onChanged }}
              paintCount={v.paints.length}
              onAssignPaints={() => onAssignPaints(models, v.name)}
            />
          ))}
        </SwapSection>
      )}

      <SwapSection
        icon={<Volume2 className="size-3.5" strokeWidth={1.75} />}
        label={t("locker.sounds")}
        hint={soundCount <= 1 ? t("locker.onlyStock") : undefined}
      >
        {sounds.variants.map((v) => {
          // Models that pull this sound in when activated (a sound may back several).
          const boundModels = Object.entries(sounds.bindings)
            .filter(([, s]) => s.toLowerCase() === v.name.toLowerCase())
            .map(([m]) => m);
          return (
            <VariantButton
              key={v.name}
              bike={bike}
              variant={v}
              kind="sound"
              busy={busy}
              disabled={disabled}
              boundModels={boundModels}
              onClick={() => onSoundSwap(bike, v.name)}
            />
          );
        })}
      </SwapSection>

      {/* Bind the active sound to the active model (case 4) — needs a model to tie to. */}
      {models && (
        <BindControl
          bike={bike}
          model={sounds.activeModel}
          sound={sounds.active}
          bound={
            sounds.bindings[sounds.activeModel]?.toLowerCase() === sounds.active.toLowerCase()
          }
          disabled={disabled}
          onBind={onBind}
          onUnbind={onUnbind}
        />
      )}
    </section>
  );
}

function SwapSection({
  icon,
  label,
  hint,
  children,
}: {
  icon: React.ReactNode;
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        <span className="text-foreground/40">{icon}</span>
        {label}
        {hint && <span className="normal-case tracking-normal text-faint">· {hint}</span>}
      </div>
      <div className="grid grid-cols-2 gap-2">{children}</div>
    </div>
  );
}

function VariantButton({
  bike,
  variant: v,
  kind,
  busy,
  disabled,
  boundModels = [],
  onClick,
  onPreview,
  paintCount = 0,
  onAssignPaints,
  manage,
}: {
  bike: string;
  variant: ModelVariant | SoundVariant;
  kind: "model" | "sound";
  busy: boolean;
  disabled: boolean;
  boundModels?: string[];
  onClick: () => void;
  /** Show this set in 3D without switching to it. Models only. */
  onPreview?: () => void;
  /** How many liveries this model claims. Models only. */
  paintCount?: number;
  /** Choose which liveries belong to this model. Models only. */
  onAssignPaints?: () => void;
  /** Move / delete this model set. Models only — a sound set is managed elsewhere. */
  manage?: { bike: string; onChanged: () => void };
}) {
  const t = useT();
  const { shareFiles } = useShare();
  // An empty set is a state, not a folder — "no model", the game's own model, or the built-in
  // engine sound. There is nothing on disk to hand anyone.
  const shareable = !v.empty;
  // A model row named "Stock" is the game's own model, packed in the bike's `.pkz` —
  // reached by clearing the loose set, so it's empty like a "no model" row but means the
  // opposite. Only the wording differs.
  const isStockModel = kind === "model" && isStockRow(v);
  const emptyLabel = isStockModel
    ? t("locker.stockModel")
    : kind === "model"
      ? t("locker.noModel")
      : t("locker.stock");
  const emptyTitle = isStockModel
    ? t("locker.switchToStockModel")
    : kind === "model"
      ? t("locker.switchToNoModel")
      : t("locker.switchToStock");
  // An empty set is applicable (revert to no-model / Stock); a set with files but
  // missing its required file is incomplete and stays disabled.
  const applicable = v.valid || v.empty;
  const selectable = !v.active && applicable && !disabled;
  const what = v.active
    ? kind === "model"
      ? t("locker.activeModel")
      : t("locker.activeSound")
    : v.empty
      ? emptyTitle
      : !v.valid
        ? kind === "model"
          ? t("locker.missingModelEdf")
          : t("locker.missingSoundFiles")
        : t("locker.switchTo", { name: v.name });

  const button = (
    <div
      className={cn(
        "flex items-center gap-1 rounded-lg border pr-1.5 transition-colors",
        v.active
          ? "border-primary/60 bg-primary/10"
          : applicable
            ? "border-white/[0.07] hover:border-white/20"
            : "border-white/[0.05] opacity-50",
        disabled && !v.active && "pointer-events-none opacity-60",
      )}
    >
      <button
        disabled={!selectable}
        onClick={onClick}
        // The share lives on a right-click, so the hover is where it gets announced.
        title={shareable ? `${what} · ${t("share.rightClickHint")}` : what}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-2 rounded-l-lg px-3 py-2.5 text-left",
          selectable && "cursor-pointer",
        )}
      >
        <span className="flex size-4 flex-none items-center justify-center">
          {v.active ? (
            busy ? (
              <Loader2 className="size-3.5 animate-spin text-primary" />
            ) : (
              <Check className="size-4 text-primary" />
            )
          ) : v.empty ? (
            <Ban className="size-3.5 text-muted-foreground" />
          ) : !v.valid ? (
            <AlertTriangle className="size-3.5 text-amber-500/80" />
          ) : busy ? (
            <Loader2 className="size-3.5 animate-spin text-muted-foreground" />
          ) : null}
        </span>
        <span className="min-w-0 flex-1">
          <span
            className={cn(
              "block truncate text-[12.5px] font-medium",
              v.active ? "text-foreground" : "text-foreground/90",
            )}
          >
            {v.name}
          </span>
          <span className="flex items-center gap-1 text-[10.5px] text-faint">
            {v.active
              ? t("common.active")
              : v.empty
                ? emptyLabel
                : t("swaps.fileCount", { count: v.fileCount })}
            {paintCount > 0 && (
              <span
                className="flex items-center gap-0.5 text-foreground/45"
                title={t("locker.paintsClaimed", { count: paintCount })}
              >
                <Palette className="size-3" />
                {paintCount}
              </span>
            )}
            {boundModels.length > 0 && (
              <span
                className="flex items-center gap-0.5 text-primary/70"
                title={t("locker.tiedToModel", { models: boundModels.join(", ") })}
              >
                <Link2 className="size-3" />
                {boundModels.join(", ")}
              </span>
            )}
          </span>
        </span>
      </button>
      {/* Siblings of the swap button, never nested in it: opening the livery picker or a
          preview must not switch the model by accident, and a button inside a button isn't
          valid markup either. */}
      {onAssignPaints && (
        <button
          onClick={onAssignPaints}
          title={t("locker.assignPaints", { name: v.name })}
          className={cn(
            "flex flex-none items-center gap-1.5 rounded-md px-2 py-1.5 text-[11px] transition-colors hover:bg-white/[0.07] hover:text-foreground",
            paintCount > 0 ? "text-foreground/60" : "text-muted-foreground",
          )}
        >
          <Palette className="size-3.5" />
          {t("locker.paints")}
        </button>
      )}
      {onPreview && (
        <button
          onClick={onPreview}
          title={t("locker.preview3d", { name: v.name })}
          className="flex flex-none items-center gap-1.5 rounded-md px-2 py-1.5 text-[11px] text-muted-foreground transition-colors hover:bg-white/[0.07] hover:text-foreground"
        >
          <Box className="size-3.5" />
          {t("locker.view3d")}
        </button>
      )}
      {manage && kind === "model" && (
        <ModelSwapActions
          bike={manage.bike}
          variant={v as ModelVariant}
          onChanged={manage.onChanged}
          className="px-1.5 py-1.5"
        />
      )}
    </div>
  );

  // A right-click shares the set, the way the Library shares anything else it lists. It
  // rides a context menu rather than a second control because the tile is itself a button,
  // and the grid has no room for another one.
  //
  // The trigger is the wrapper, not the button: the *active* variant's button is disabled —
  // and a disabled button fires no mouse events at all — which would have left the one set
  // most people want to hand over as the one that couldn't be right-clicked.
  if (!shareable) return button;
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="min-w-0">{button}</div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={() => shareFiles([variantRel(bike, kind, v.name)])}>
          <Share2 className="size-4" /> {t("share.action")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

function BindControl({
  bike,
  model,
  sound,
  bound,
  disabled,
  onBind,
  onUnbind,
}: {
  bike: string;
  model: string;
  sound: string;
  bound: boolean;
  disabled: boolean;
  onBind: (bike: string, model: string, sound: string) => void;
  onUnbind: (bike: string, model: string, sound: string) => void;
}) {
  const t = useT();
  return (
    <button
      disabled={disabled}
      onClick={() => (bound ? onUnbind(bike, model, sound) : onBind(bike, model, sound))}
      className={cn(
        "flex items-center gap-1.5 self-start rounded-lg border px-2.5 py-1.5 text-[11px] transition-colors",
        bound
          ? "border-primary/40 bg-primary/[0.07] text-primary/90 hover:border-primary/60"
          : "border-white/[0.07] text-muted-foreground hover:border-white/20 hover:text-foreground",
        disabled && "pointer-events-none opacity-60",
      )}
      title={
        bound
          ? t("locker.boundHint", { sound, model })
          : t("locker.unboundHint", { sound, model })
      }
    >
      {bound ? <Link2Off className="size-3.5" /> : <Link2 className="size-3.5" />}
      {bound
        ? t("locker.untieAction", { sound, model })
        : t("locker.tieAction", { sound, model })}
    </button>
  );
}
