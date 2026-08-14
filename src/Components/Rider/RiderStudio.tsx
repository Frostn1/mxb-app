import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCw, AlertTriangle, Save, Loader2, FolderInput } from "lucide-react";
import { toast } from "sonner";
import { useT, type TKey } from "../../i18n/context";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import type { Loadout, RiderPart } from "../../types";
import {
  presetsSave,
  scanGearRepairs,
  repairGear,
  type GearRepair,
} from "../../api/mods";
import { ViewerPanel } from "../Viewer/ViewerPanel";
import { SlotField } from "../Presets/SlotField";
import {
  SLOTS,
  SLOT_GROUPS,
  EMPTY_LOADOUT,
  loadScans,
  type Scans,
} from "../../lib/presets";
import { useGearPaints } from "../../lib/useGearPaints";

const RIDER_GROUPS = SLOT_GROUPS.filter((g) => g.id !== "bike");

const TOGGLES: { part: RiderPart["part"]; label: TKey }[] = [
  { part: "helmet", label: "category.helmet" },
  { part: "protection", label: "category.protection" },
  { part: "boots", label: "category.boots" },
];

interface RiderStudioProps {
  initialLoadout?: Loadout | null;
  onLoaded?: () => void;
}

export default function RiderStudio({ initialLoadout, onLoaded }: RiderStudioProps) {
  const t = useT();
  const [scans, setScans] = useState<Scans | null>(null);
  const [loadout, setLoadout] = useState<Loadout>(EMPTY_LOADOUT);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  // Nothing hidden to start. Protection used to be, back when it came out of the loader as
  // a grey blob scaled to the whole torso — with the piece drawn as authored there's no
  // reason to hide the slot the rider tab is most often opened for.
  const [hidden, setHidden] = useState<RiderPart["part"][]>([]);
  const [error, setError] = useState<string | null>(null);
  // Gear the game can't reach where it was installed — loose in an area root, or packaged and
  // buried a folder deep. See `gearrepair` on the Rust side. Surfaced here because this is the
  // tab where the damage shows: the model is missing from its picker, or listed under the
  // download's slug and rendering nothing.
  const [repairs, setRepairs] = useState<GearRepair[]>([]);
  const [repairing, setRepairing] = useState<string | null>(null);
  // Paints the chosen models carry, merged with the loose ones the scan found.
  const { optionsFor, missingFor } = useGearPaints(loadout);

  const setSlot = useCallback((key: keyof Loadout, value: string) => {
    setLoadout((prev) => ({ ...prev, [key]: value }));
  }, []);

  const toggle = useCallback((part: RiderPart["part"]) => {
    setHidden((prev) =>
      prev.includes(part) ? prev.filter((p) => p !== part) : [...prev, part],
    );
  }, []);

  useEffect(() => {
    if (initialLoadout) {
      setLoadout(initialLoadout);
      onLoaded?.();
    }
  }, [initialLoadout, onLoaded]);

  const onSave = useCallback(async () => {
    const nm = name.trim();
    if (!nm) {
      toast.error(t("rider.nameFirst"));
      return;
    }
    setBusy(true);
    try {
      await presetsSave({ name: nm, loadout });
      setName("");
      toast.success(`Saved “${nm}” — apply it from the Presets tab.`);
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [name, loadout, t]);

  const load = useCallback(async () => {
    setError(null);
    try {
      const sc = await loadScans();
      setScans(sc);
      // Kit, gloves and profile goggles are all looked up by rider profile. Presets gets
      // one for free from the captured loadout; a fresh Rider tab has none, which left
      // those slots empty. Seed the first installed profile unless one is already set.
      setLoadout((prev) =>
        prev.rider || !sc.riderProfiles.length ? prev : { ...prev, rider: sc.riderProfiles[0] },
      );
    } catch (e) {
      setError(String(e));
    }
    // Never fatal: a mods folder that can't be inspected for this is still a mods folder,
    // and the tab has to open either way.
    setRepairs(await scanGearRepairs().catch(() => []));
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const onRepair = useCallback(
    async (r: GearRepair) => {
      setRepairing(r.id);
      try {
        const moved = await repairGear(r.id);
        const done: TKey = r.kind === "unwrap" ? "rider.unwrapDone" : "rider.repairDone";
        toast.success(
          moved
            ? t(done, { count: moved, model: r.model })
            : t("rider.repairNothing"),
        );
        // Re-scan rather than dropping the banner locally: gathering changes what the
        // pickers can offer, and the model that was invisible a moment ago is the whole
        // point of having done it.
        await load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setRepairing(null);
      }
    },
    [load, t],
  );

  const grouped = useMemo(
    () => RIDER_GROUPS.map((g) => ({ ...g, slots: SLOTS.filter((s) => s.group === g.id) })),
    [],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5">
        <div className="ml-auto flex items-center gap-2">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("rider.namePlaceholder")}
            className="h-8 w-[180px]"
            onKeyDown={(e) => e.key === "Enter" && void onSave()}
          />
          <Button size="sm" onClick={() => void onSave()} disabled={busy}>
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
            Save rider
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void load()}>
            <RefreshCw className="size-3.5" />
            Refresh
          </Button>
        </div>
      </header>

      {error && (
        <div className="mx-7 mb-3 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12.5px] text-destructive">
          <AlertTriangle className="size-4" />
          {error}
        </div>
      )}

      {repairs.map((r) => (
        <div
          key={r.id}
          className="mx-7 mb-3 flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2.5 text-[12.5px]"
        >
          <FolderInput className="mt-0.5 size-4 flex-none text-amber-500" />
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="font-semibold">
              {t(r.kind === "unwrap" ? "rider.unwrapTitle" : "rider.repairTitle", {
                area: r.area,
              })}
            </span>
            <span className="text-muted-foreground">
              {r.kind === "unwrap"
                ? t("rider.unwrapBody", {
                    area: r.area,
                    model: r.model,
                    // The id carries where it's buried; the folder is the part the person
                    // clicking recognises, since it's what the picker has been showing them.
                    folder: r.id.slice(r.area.length + 1),
                  })
                : t("rider.repairBody", { area: r.area, model: r.model })}
            </span>
            {/* The exact list, because this moves files on disk and the person clicking
                should be able to see what it will touch before it does. */}
            <span className="truncate text-[11px] text-faint" title={r.items.join(", ")}>
              {r.items.join(", ")}
            </span>
          </div>
          <Button
            size="sm"
            variant="outline"
            className="flex-none"
            disabled={repairing !== null}
            onClick={() => void onRepair(r)}
          >
            {repairing === r.id ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <FolderInput className="size-3.5" />
            )}
            {t("rider.repairAction")}
          </Button>
        </div>
      ))}

      <div className="flex min-h-0 flex-1 gap-5 overflow-hidden px-7 pb-6">
        {/* Picker column */}
        <section className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
          {/* Show-on-model toggles */}
          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-xl border border-white/[0.07] bg-card/40 p-3.5">
            <span className="text-[11px] font-semibold uppercase tracking-wide text-faint">
              {t("rider.showOnModel")}
            </span>
            {TOGGLES.map(({ part, label }) => (
              <label key={part} className="flex items-center gap-2 text-[12px] text-muted-foreground">
                <Switch checked={!hidden.includes(part)} onCheckedChange={() => toggle(part)} />
                {t(label)}
              </label>
            ))}
          </div>

          {/* Rider slot groups */}
          {grouped.map((g) => (
            <div key={g.id} className="flex flex-col gap-1.5">
              <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
                {t(g.label)}
              </h2>
              <div className="grid grid-cols-1 gap-x-3.5 gap-y-2 sm:grid-cols-2">
                {g.slots.map((slot) => (
                  <SlotField
                    key={slot.key}
                    slot={slot}
                    value={loadout[slot.key]}
                    options={optionsFor(slot, "", scans)}
                    missing={missingFor(slot, "", scans)}
                    compact
                    onChange={(v) => setSlot(slot.key, v)}
                  />
                ))}
              </div>
            </div>
          ))}
        </section>

        {/* Live rider render */}
        <ViewerPanel loadout={loadout} riderOnly hiddenParts={hidden} className="w-[420px] flex-none" />
      </div>
    </div>
  );
}
