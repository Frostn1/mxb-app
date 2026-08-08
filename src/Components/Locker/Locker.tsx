import { useCallback, useEffect, useState } from "react";
import {
  Bike,
  Volume2,
  Check,
  RefreshCw,
  Loader2,
  AlertTriangle,
  Ban,
  FolderInput,
  Link2,
  Link2Off,
  Wrench,
  Palette,
} from "lucide-react";
import { toast } from "sonner";
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
 * Trailing feedback for a swap toast.
 *
 * Models and sounds get different notes because they refresh by different routes.
 * `live_refresh` re-runs the game's *customization* loader — that reloads paints and
 * gear but never the bike mesh, so it says nothing about whether a swapped model is
 * visible. A model only appears live if FrostMod re-applies the bike (`model_refresh`),
 * which it does solely for the bike you currently have selected.
 */
function swapNote(kind: "model" | "sound", outcome: SwapApplyOutcome): string {
  if (!outcome.game_running) return "Loads next time the game opens.";

  if (kind === "model") {
    switch (outcome.model_refresh) {
      case "signaled":
        return "Refreshing in-game — if it's your selected bike, it changes now.";
      case "not_running":
        return "Run FrostMod to see model swaps live — for now, reselect the bike in-game.";
      case "write_failed":
        return "Couldn't reach FrostMod — reselect the bike in-game to load it.";
      case "unsupported":
        return "Live model refresh is Windows-only — reselect the bike in-game.";
      default: // null — instant refresh is switched off in Settings
        return "Reselect the bike in MX Bikes to load it (instant refresh is off).";
    }
  }

  switch (outcome.live_refresh) {
    case "refreshed":
      return "Refreshed live in-game.";
    case "failed":
      return "Instant refresh failed — reselect your profile in-game to load it.";
    default:
      return "Reselect your profile in MX Bikes to load the swap.";
  }
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
  const [rows, setRows] = useState<Row[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bike name currently being mutated (disables its rows + spins the target).
  const [busy, setBusy] = useState<string | null>(null);
  // Model sets found sitting loose outside `FrostMod Models/` (banner + dialog).
  const [loose, setLoose] = useState<LooseSwapBike[]>([]);
  const [registerOpen, setRegisterOpen] = useState(false);
  // Bikes gutted by a pre-0.6.3 swap — their setup files are in a swap folder.
  const [orphaned, setOrphaned] = useState<OrphanedSetup[]>([]);
  // The model swap whose liveries are being assigned, if the dialog is open.
  const [assigning, setAssigning] = useState<{ bike: string; model: string } | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [models, sounds, detected, broken] = await Promise.all([
        scanModelSwaps(),
        scanSoundSwaps().catch(() => [] as BikeSounds[]),
        detectLooseSwaps().catch(() => [] as LooseSwapBike[]),
        detectOrphanedSetup().catch(() => [] as OrphanedSetup[]),
      ]);
      setRows(mergeRows(models, sounds));
      setLoose(detected);
      setOrphaned(broken);
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
  // Re-read from `rows` rather than snapshotting, so a save's refresh reaches the dialog.
  const assigningModels =
    (assigning && rows?.find((r) => r.bike === assigning.bike)?.models) || null;

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

  const onRepair = (bike: string) =>
    run(bike, `Restored ${bike}'s setup files.`, () => repairOrphanedSetup(bike), (n) =>
      `${n} file${n === 1 ? "" : "s"} put back — the bike should load again.`,
    );

  const onModelSwap = (bike: string, target: string) =>
    run(bike, `Switched ${bike} model to “${target}”.`, () => applyModelSwap(bike, target), (r) =>
      swapNote("model", r),
    );
  const onSoundSwap = (bike: string, target: string) =>
    run(bike, `Switched ${bike} sound to “${target}”.`, () => applySoundSwap(bike, target), (r) =>
      swapNote("sound", r),
    );
  const onBind = (bike: string, model: string, sound: string) =>
    run(bike, `Tied “${sound}” to model “${model}”.`, () => bindSound(bike, model, sound));
  const onUnbind = (bike: string, model: string, sound: string) =>
    run(bike, `Untied “${sound}” from model “${model}”.`, () => unbindSound(bike, model));

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">Locker</h1>
          <HelpHint
            title="Locker"
            description="Swap each bike's model and engine sound between the sets you've installed."
          />
        </div>
        <button
          onClick={() => void load()}
          className="ml-auto flex items-center gap-1.5 rounded-lg border border-input bg-card px-3 py-2 text-[12.5px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <RefreshCw className={cn("size-3.5", rows === null && "animate-spin")} />
          Rescan
        </button>
      </header>

      {orphaned.map((o) => (
        <div
          key={o.bike}
          className="mx-7 mb-3.5 flex items-center gap-2.5 rounded-lg border border-destructive/30 bg-destructive/[0.07] px-3.5 py-2.5"
        >
          <Wrench className="size-4 flex-none text-destructive/80" />
          <span className="min-w-0 flex-1 text-[12.5px] text-foreground/90">
            <span className="font-semibold">{o.bike}</span> is missing its setup files — an
            earlier version moved them into a swap folder, which stops the bike loading
            in-game at all.{" "}
            <span className="font-mono text-faint">{o.files.join(", ")}</span>
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
            Restore
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
            {looseCount} model / sound set{looseCount === 1 ? "" : "s"} found loose in your
            bikes — register {looseCount === 1 ? "it" : "them"} into{" "}
            <span className="font-mono text-faint">FrostMod Models</span> /{" "}
            <span className="font-mono text-faint">Sounds</span>.
          </span>
          <span className="flex flex-none items-center gap-1.5 text-[12px] font-semibold text-amber-500/90">
            <FolderInput className="size-3.5" />
            Register
          </span>
        </button>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <p className="select-text py-16 text-center text-[13px] text-destructive">{error}</p>
        ) : rows === null ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">Scanning bikes…</p>
        ) : rows.length === 0 ? (
          <div className="mx-auto max-w-md py-14 text-[13px] text-muted-foreground">
            <p className="mb-3 text-center text-foreground/90">No swappable bikes yet.</p>
            <p className="mb-3">Two things have to be true before a swap can happen:</p>
            <ol className="mb-4 list-decimal space-y-2 pl-5">
              <li>
                The bike is <span className="text-foreground/90">unpacked</span> into
                <span className="mx-1 font-mono text-faint">mods/bikes/&lt;Bike&gt;/</span>—
                a packed <span className="font-mono text-faint">.pkz</span> can&apos;t be
                swapped.
                Unpack one from the Library.
              </li>
              <li>
                Each alternate model sits in its own folder inside that bike, holding a mesh
                (<span className="font-mono text-faint">.edf</span>). Drop it anywhere in the
                bike folder and hit Scan below — we&apos;ll offer to file it under
                <span className="mx-1 font-mono text-faint">FrostMod Models/</span>.
              </li>
            </ol>
            <button
              onClick={() => void load()}
              className="mx-auto flex items-center gap-1.5 rounded-lg border border-input bg-card px-3 py-2 text-[12.5px] transition-colors hover:text-foreground"
            >
              <RefreshCw className="size-3.5" />
              Scan for swaps
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
                onBind={onBind}
                onUnbind={onUnbind}
                onAssignPaints={(model) => setAssigning({ bike: r.bike, model })}
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

      {assigning && assigningModels && (
        <AssignPaintsDialog
          key={`${assigning.bike}:${assigning.model}`}
          open
          onOpenChange={(o) => !o && setAssigning(null)}
          bike={assigning.bike}
          model={assigning.model}
          models={assigningModels}
          onDone={() => void load()}
        />
      )}
    </div>
  );
}

function BikeCard({
  row,
  busy,
  disabled,
  onModelSwap,
  onSoundSwap,
  onBind,
  onUnbind,
  onAssignPaints,
}: {
  row: Row;
  busy: boolean;
  disabled: boolean;
  onModelSwap: (bike: string, target: string) => void;
  onSoundSwap: (bike: string, target: string) => void;
  onBind: (bike: string, model: string, sound: string) => void;
  onUnbind: (bike: string, model: string, sound: string) => void;
  onAssignPaints: (model: string) => void;
}) {
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
            {models ? `model “${models.active}”` : "no model swaps"} · sound “{sounds.active}”
          </div>
        </div>
      </div>

      {models && (
        <SwapSection
          icon={<Bike className="size-3.5" strokeWidth={1.75} />}
          label="Models"
          hint={modelCount <= 1 ? "Only one model — install more to swap" : undefined}
        >
          {models.variants.map((v) => (
            <div key={v.name} className="flex items-stretch gap-1.5">
              <VariantButton
                variant={v}
                kind="model"
                busy={busy}
                disabled={disabled}
                className="min-w-0 flex-1"
                onClick={() => onModelSwap(bike, v.name)}
              />
              <PaintsButton
                count={v.paints.length}
                model={v.name}
                disabled={disabled}
                onClick={() => onAssignPaints(v.name)}
              />
            </div>
          ))}
        </SwapSection>
      )}

      <SwapSection
        icon={<Volume2 className="size-3.5" strokeWidth={1.75} />}
        label="Sounds"
        hint={soundCount <= 1 ? "Only Stock — install a sound mod to swap" : undefined}
      >
        {sounds.variants.map((v) => {
          // Models that pull this sound in when activated (a sound may back several).
          const boundModels = Object.entries(sounds.bindings)
            .filter(([, s]) => s.toLowerCase() === v.name.toLowerCase())
            .map(([m]) => m);
          return (
            <VariantButton
              key={v.name}
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

/**
 * Opens the livery assignment for one model swap. A separate control from the swap button
 * beside it — assigning is not switching, and a button can't live inside a button.
 */
function PaintsButton({
  count,
  model,
  disabled,
  onClick,
}: {
  count: number;
  model: string;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={
        count > 0
          ? `${count} liver${count === 1 ? "y" : "ies"} assigned to “${model}” — only these show while it's active`
          : `Pick the liveries drawn for “${model}”, so the others don't show while it's active`
      }
      className={cn(
        "flex flex-none flex-col items-center justify-center gap-0.5 rounded-lg border px-2.5 transition-colors",
        count > 0
          ? "border-primary/30 bg-primary/[0.06] text-primary/80 hover:border-primary/50"
          : "border-white/[0.07] text-muted-foreground hover:border-white/20 hover:text-foreground",
        disabled && "pointer-events-none opacity-60",
      )}
    >
      <Palette className="size-3.5" strokeWidth={1.75} />
      {count > 0 && <span className="text-[10px] font-semibold leading-none">{count}</span>}
    </button>
  );
}

function VariantButton({
  variant: v,
  kind,
  busy,
  disabled,
  boundModels = [],
  className,
  onClick,
}: {
  variant: ModelVariant | SoundVariant;
  kind: "model" | "sound";
  busy: boolean;
  disabled: boolean;
  boundModels?: string[];
  className?: string;
  onClick: () => void;
}) {
  const emptyLabel = kind === "model" ? "No model" : "Stock";
  // An empty set is applicable (revert to no-model / Stock); a set with files but
  // missing its required file is incomplete and stays disabled.
  const applicable = v.valid || v.empty;
  const selectable = !v.active && applicable && !disabled;
  return (
    <button
      disabled={!selectable}
      onClick={onClick}
      title={
        v.active
          ? `Active ${kind}`
          : v.empty
            ? kind === "model"
              ? "Switch to no model — removes the current model files"
              : "Switch to Stock — removes the sound mod (built-in sound plays)"
            : !v.valid
              ? kind === "model"
                ? "This set has no model.edf"
                : "This set is missing engine.scl or sfx.cfg"
              : `Switch to ${v.name}`
      }
      className={cn(
        "flex items-center gap-2 rounded-lg border px-3 py-2.5 text-left transition-colors",
        v.active
          ? "border-primary/60 bg-primary/10"
          : applicable
            ? "cursor-pointer border-white/[0.07] hover:border-white/20"
            : "border-white/[0.05] opacity-50",
        disabled && !v.active && "pointer-events-none opacity-60",
        className,
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
            ? "Active"
            : v.empty
              ? emptyLabel
              : `${v.fileCount} file${v.fileCount === 1 ? "" : "s"}`}
          {boundModels.length > 0 && (
            <span
              className="flex items-center gap-0.5 text-primary/70"
              title={`Tied to model ${boundModels.join(", ")}`}
            >
              <Link2 className="size-3" />
              {boundModels.join(", ")}
            </span>
          )}
        </span>
      </span>
    </button>
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
          ? `“${sound}” is tied to model “${model}” — it travels with that model. Click to untie.`
          : `Tie the active sound “${sound}” to model “${model}” so switching to it pulls the sound in.`
      }
    >
      {bound ? <Link2Off className="size-3.5" /> : <Link2 className="size-3.5" />}
      {bound ? `Untie “${sound}” from “${model}”` : `Tie “${sound}” to “${model}”`}
    </button>
  );
}
