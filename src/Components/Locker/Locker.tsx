import { useCallback, useEffect, useState } from "react";
import { Bike, Check, RefreshCw, Loader2, AlertTriangle } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { scanModelSwaps, applyModelSwap } from "../../api/mods";
import type { BikeModels } from "../../types";

/**
 * Locker — the app-side bike **model swap** manager, twinned with FrostMod's
 * in-game swapper (F8 menu > 3). For each *extracted* bike it lists the model
 * sets you have (under `<Bike>/FrostMod Models/`), marks the active one, and lets
 * you switch — the same backup-current / move-in file dance FrostMod does, so the
 * two stay interchangeable. Packed `.pkz` bikes have no swappable model and don't
 * appear here (extract them first).
 */
export default function Locker() {
  const [bikes, setBikes] = useState<BikeModels[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bike name currently being swapped (disables its rows + spins the target).
  const [applying, setApplying] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setBikes(await scanModelSwaps());
    } catch (e) {
      setError(String(e));
      setBikes([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const onSwap = useCallback(
    async (bike: string, target: string) => {
      setApplying(bike);
      try {
        await applyModelSwap(bike, target);
        toast.success(`Switched ${bike} to “${target}”.`);
        await load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setApplying(null);
      }
    },
    [load],
  );

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">Model Swaps</h1>
        <p className="text-[12.5px] text-muted-foreground">
          Swap each bike’s model between the sets you’ve installed.
        </p>
        <button
          onClick={() => void load()}
          className="ml-auto flex items-center gap-1.5 rounded-lg border border-input bg-card px-3 py-2 text-[12.5px] text-muted-foreground transition-colors hover:text-foreground"
        >
          <RefreshCw className={cn("size-3.5", bikes === null && "animate-spin")} />
          Rescan
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <p className="select-text py-16 text-center text-[13px] text-destructive">
            {error}
          </p>
        ) : bikes === null ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            Scanning bikes…
          </p>
        ) : bikes.length === 0 ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            No extracted bikes found. Model swaps need a bike unpacked into
            <span className="mx-1 font-mono text-faint">mods/bikes/&lt;Bike&gt;/</span>
            (a loose <span className="font-mono text-faint">model.edf</span>).
          </p>
        ) : (
          <div className="flex flex-col gap-4">
            {bikes.map((b) => (
              <BikeCard
                key={b.bike}
                data={b}
                busy={applying === b.bike}
                disabled={applying !== null}
                onSwap={onSwap}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function BikeCard({
  data,
  busy,
  disabled,
  onSwap,
}: {
  data: BikeModels;
  busy: boolean;
  disabled: boolean;
  onSwap: (bike: string, target: string) => void;
}) {
  const single = data.variants.length <= 1;
  return (
    <section className="flex flex-col gap-2.5 rounded-xl border border-white/[0.07] bg-card p-4">
      <div className="flex items-center gap-2.5">
        <div className="grid size-8 flex-none place-items-center rounded-md bg-foreground/[0.06] text-foreground/40">
          <Bike className="size-4" strokeWidth={1.5} />
        </div>
        <div className="min-w-0">
          <div className="truncate text-[14px] font-semibold">{data.bike}</div>
          <div className="truncate text-[11px] text-muted-foreground">
            {single
              ? "Only one model — install more to swap"
              : `${data.variants.length} models · active “${data.active}”`}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        {data.variants.map((v) => {
          const selectable = !v.active && v.valid && !disabled;
          return (
            <button
              key={v.name}
              disabled={!selectable}
              onClick={() => onSwap(data.bike, v.name)}
              title={
                v.active
                  ? "Active model"
                  : !v.valid
                    ? "This set has no model.edf"
                    : `Switch to ${v.name}`
              }
              className={cn(
                "flex items-center gap-2 rounded-lg border px-3 py-2.5 text-left transition-colors",
                v.active
                  ? "border-primary/60 bg-primary/10"
                  : v.valid
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
                <span className="block text-[10.5px] text-faint">
                  {v.active ? "Active" : `${v.fileCount} file${v.fileCount === 1 ? "" : "s"}`}
                </span>
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
