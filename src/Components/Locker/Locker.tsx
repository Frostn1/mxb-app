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
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import {
  scanModelSwaps,
  applyModelSwap,
  detectLooseSwaps,
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
  SoundVariant,
} from "../../types";
import RegisterSwapsDialog from "./RegisterSwapsDialog";

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

  const load = useCallback(async () => {
    setError(null);
    try {
      const [models, sounds, detected] = await Promise.all([
        scanModelSwaps(),
        scanSoundSwaps().catch(() => [] as BikeSounds[]),
        detectLooseSwaps().catch(() => [] as LooseSwapBike[]),
      ]);
      setRows(mergeRows(models, sounds));
      setLoose(detected);
    } catch (e) {
      setError(String(e));
      setRows([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const looseCount = loose.reduce((n, b) => n + b.candidates.length, 0);

  // Runs a mutation for `bike`, toasting success/failure and refreshing every scan.
  const run = useCallback(
    async (bike: string, ok: string, fn: () => Promise<void>) => {
      setBusy(bike);
      try {
        await fn();
        toast.success(ok);
        await load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(null);
      }
    },
    [load],
  );

  const onModelSwap = (bike: string, target: string) =>
    run(bike, `Switched ${bike} model to “${target}”.`, () => applyModelSwap(bike, target));
  const onSoundSwap = (bike: string, target: string) =>
    run(bike, `Switched ${bike} sound to “${target}”.`, () => applySoundSwap(bike, target));
  const onBind = (bike: string, model: string, sound: string) =>
    run(bike, `Tied “${sound}” to model “${model}”.`, () => bindSound(bike, model, sound));
  const onUnbind = (bike: string, model: string, sound: string) =>
    run(bike, `Untied “${sound}” from model “${model}”.`, () => unbindSound(bike, model));

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">Locker</h1>
        <p className="text-[12.5px] text-muted-foreground">
          Swap each bike’s model and engine sound between the sets you’ve installed.
        </p>
        <button
          onClick={() => void load()}
          className="ml-auto flex items-center gap-1.5 rounded-lg border border-input bg-card px-3 py-2 text-[12.5px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <RefreshCw className={cn("size-3.5", rows === null && "animate-spin")} />
          Rescan
        </button>
      </header>

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
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            No extracted bikes found. Swaps need a bike unpacked into
            <span className="mx-1 font-mono text-faint">mods/bikes/&lt;Bike&gt;/</span>
            (a loose <span className="font-mono text-faint">model.edf</span> or sound files).
          </p>
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
}: {
  row: Row;
  busy: boolean;
  disabled: boolean;
  onModelSwap: (bike: string, target: string) => void;
  onSoundSwap: (bike: string, target: string) => void;
  onBind: (bike: string, model: string, sound: string) => void;
  onUnbind: (bike: string, model: string, sound: string) => void;
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
            <VariantButton
              key={v.name}
              variant={v}
              kind="model"
              busy={busy}
              disabled={disabled}
              onClick={() => onModelSwap(bike, v.name)}
            />
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

function VariantButton({
  variant: v,
  kind,
  busy,
  disabled,
  boundModels = [],
  onClick,
}: {
  variant: ModelVariant | SoundVariant;
  kind: "model" | "sound";
  busy: boolean;
  disabled: boolean;
  boundModels?: string[];
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
