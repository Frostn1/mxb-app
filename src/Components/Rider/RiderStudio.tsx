import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCw, AlertTriangle, Save, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import type { GearPaints, Loadout, RiderPart } from "../../types";
import { listInstalledGearPaints, presetsSave } from "../../api/mods";
import { ViewerPanel } from "../Viewer/ViewerPanel";
import { SlotField } from "../Presets/SlotField";
import {
  SLOTS,
  SLOT_GROUPS,
  EMPTY_LOADOUT,
  loadScans,
  slotOptions,
  type Scans,
  type SlotDef,
} from "../../lib/presets";

const EMPTY_GEAR_PAINTS: GearPaints = { paints: [], goggles: [] };

const RIDER_GROUPS = SLOT_GROUPS.filter((g) => g.id !== "bike");

const TOGGLES: { part: RiderPart["part"]; label: string }[] = [
  { part: "helmet", label: "Helmet" },
  { part: "protection", label: "Protection" },
  { part: "boots", label: "Boots" },
];

interface RiderStudioProps {
  initialLoadout?: Loadout | null;
  onLoaded?: () => void;
}

export default function RiderStudio({ initialLoadout, onLoaded }: RiderStudioProps) {
  const [scans, setScans] = useState<Scans | null>(null);
  const [loadout, setLoadout] = useState<Loadout>(EMPTY_LOADOUT);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [hidden, setHidden] = useState<RiderPart["part"][]>(["protection"]);
  const [error, setError] = useState<string | null>(null);
  const [gearPaints, setGearPaints] = useState<Record<"helmet" | "boots" | "protection", GearPaints>>({
    helmet: EMPTY_GEAR_PAINTS,
    boots: EMPTY_GEAR_PAINTS,
    protection: EMPTY_GEAR_PAINTS,
  });

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
      toast.error("Name this rider look first.");
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
  }, [name, loadout]);

  const load = useCallback(async () => {
    setError(null);
    try {
      setScans(await loadScans());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let alive = true;
    const grab = (part: RiderPart["part"], model: string) =>
      listInstalledGearPaints(part, model).catch(() => EMPTY_GEAR_PAINTS);
    Promise.all([
      grab("helmet", loadout.helmet),
      grab("boots", loadout.boots),
      grab("protection", loadout.protection),
    ]).then(([helmet, boots, protection]) => {
      if (alive) setGearPaints({ helmet, boots, protection });
    });
    return () => {
      alive = false;
    };
  }, [loadout.helmet, loadout.boots, loadout.protection]);

  const packedFor = useCallback(
    (key: keyof Loadout): string[] => {
      switch (key) {
        case "helmetPaint":
          return gearPaints.helmet.paints;
        case "gogglesPaint":
          return gearPaints.helmet.goggles;
        case "bootsPaint":
          return gearPaints.boots.paints;
        case "protectionPaint":
          return gearPaints.protection.paints;
        default:
          return [];
      }
    },
    [gearPaints],
  );

  const optionsFor = useCallback(
    (slot: SlotDef): string[] => {
      const base = scans ? slotOptions(slot, "", loadout, scans) : [];
      return [...new Set([...base, ...packedFor(slot.key)])];
    },
    [scans, loadout, packedFor],
  );

  const missingFor = useCallback(
    (slot: SlotDef): boolean => {
      const val = loadout[slot.key];
      if (slot.freeText || !val) return false;
      return !optionsFor(slot).includes(val);
    },
    [loadout, optionsFor],
  );

  const grouped = useMemo(
    () => RIDER_GROUPS.map((g) => ({ ...g, slots: SLOTS.filter((s) => s.group === g.id) })),
    [],
  );

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">Rider</h1>
        <p className="hidden text-[12.5px] text-muted-foreground lg:block">
          Dress the player model — helmet, goggles, outfit and boots together.
        </p>
        <div className="ml-auto flex items-center gap-2">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name this rider…"
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

      <div className="flex min-h-0 flex-1 gap-5 overflow-hidden px-7 pb-6">
        {/* Picker column */}
        <section className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
          {/* Show-on-model toggles */}
          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-xl border border-white/[0.07] bg-card/40 p-3.5">
            <span className="text-[11px] font-semibold uppercase tracking-wide text-faint">
              Show on model
            </span>
            {TOGGLES.map(({ part, label }) => (
              <label key={part} className="flex items-center gap-2 text-[12px] text-muted-foreground">
                <Switch checked={!hidden.includes(part)} onCheckedChange={() => toggle(part)} />
                {label}
              </label>
            ))}
          </div>

          {/* Rider slot groups */}
          {grouped.map((g) => (
            <div key={g.id} className="flex flex-col gap-2">
              <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
                {g.label}
              </h2>
              <div className="grid grid-cols-1 gap-x-4 gap-y-2.5 sm:grid-cols-2">
                {g.slots.map((slot) => (
                  <SlotField
                    key={slot.key}
                    slot={slot}
                    value={loadout[slot.key]}
                    options={optionsFor(slot)}
                    missing={missingFor(slot)}
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
