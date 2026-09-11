import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Card } from "@frost/shared/Components/ui/card";
import { RefreshCw, AlertTriangle, Save, Loader2, FolderInput } from "lucide-react";
import { toast } from "sonner";
import { useT, type TKey } from "@/i18n";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import type { RiderPart } from "@frost/shared/types";
import {
  presetsSave,
  scanGearRepairs,
  repairGear,
  modelSwapLineup,
  type GearRepair,
} from "@frost/shared/api/mods";
import { ViewerPanel } from "@frost/shared/Components/Viewer/ViewerPanel";
import { SlotField } from "@frost/shared/Components/SlotField";
import { SLOTS, SLOT_GROUPS, pickedModel, type SlotDef } from "@frost/shared/lib/presets";
import { useGearPaints } from "@frost/shared/lib/useGearPaints";
import { useConfig } from "@frost/shared/Context/Config";
import { Combobox } from "@frost/shared/Components/ui/combobox";
import { useRiderKit } from "./RiderKitContext";

const RIDER_GROUPS = SLOT_GROUPS.filter((g) => g.id !== "bike");

/**
 * How wide the preview column is, in px, and where that is remembered.
 *
 * Machine-local, like the app's other bits of remembered layout — a window that is 1400px
 * on the desktop and 1024 on the laptop wants a different answer on each, and neither is
 * worth a round trip to the config file.
 */
const PICKERS_W_KEY = "mxb.rider.pickersWidth:v2";
const PICKERS_W = { min: 330, max: 620, initial: 380 };
/** What the picker column keeps for itself, however far the preview is dragged. */
const PREVIEW_MIN = 380;

/**
 * The bike slots the preview actually draws.
 *
 * `bikeFont` and `tyres` are the rest of the group and neither reaches the model, so they'd
 * be two controls here that do nothing you can see. They stay in Presets, where a slot's
 * job is to be written to `profile.ini` rather than to be looked at.
 */
const BIKE_SLOTS: SlotDef[] = SLOTS.filter(
  (s) => s.key === "paint" || s.key === "modelSwap",
);

const TOGGLES: { part: RiderPart["part"]; label: TKey }[] = [
  { part: "helmet", label: "category.helmet" },
  { part: "protection", label: "category.protection" },
  { part: "boots", label: "category.boots" },
];

export default function RiderStudio() {
  const t = useT();
  // A build with no geometry decoder can't draw a real bike, and a cartoon stand-in next to
  // a real rider reads as the preset having resolved to that. Rider only there, as before.
  const { bikePreview } = useConfig();
  // The kit itself is held above this tab and the Pose tab, so the two show one rider.
  const {
    scans,
    bikes,
    loadout,
    setSlot,
    bike,
    setBike,
    hidden,
    toggleHidden,
    reload,
    error,
  } = useRiderKit();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  // Gear the game can't reach where it was installed — loose in an area root, or packaged and
  // buried a folder deep. See `gearrepair` on the Rust side. Surfaced here because this is the
  // tab where the damage shows: the model is missing from its picker, or listed under the
  // download's slug and rendering nothing.
  const [repairs, setRepairs] = useState<GearRepair[]>([]);
  const [repairing, setRepairing] = useState<string | null>(null);
  // Paints the chosen models carry, merged with the loose ones the scan found.
  const { optionsFor, missingFor } = useGearPaints(loadout);
  // The preview column's width, dragged by the handle on its left edge. A 420px window onto
  // a bike and a rider side by side is a small one, and this tab is where a look is composed.
  const row = useRef<HTMLDivElement>(null);
  const [pickersW, setPickersW] = useState(() => {
    const saved = Number(localStorage.getItem(PICKERS_W_KEY));
    return saved >= PICKERS_W.min && saved <= PICKERS_W.max ? saved : PICKERS_W.initial;
  });
  const drag = useRef<{ from: number; was: number } | null>(null);

  // Clamped against the row as it is now, not just the fixed limits: on a narrow window the
  // pickers have to stay usable, and they're the half that can't be scrolled sideways.
  const widen = useCallback((to: number) => {
    const room = (row.current?.clientWidth ?? PICKERS_W.max) - PREVIEW_MIN;
    setPickersW(Math.max(PICKERS_W.min, Math.min(to, Math.min(PICKERS_W.max, room))));
  }, []);

  const onDragMove = useCallback(
    (e: React.PointerEvent) => {
      // Dragging left grows the preview, since it's the right-hand column.
      if (drag.current) widen(drag.current.was + (drag.current.from - e.clientX));
    },
    [widen],
  );

  // Remembered wherever the width came from — the drag, the double-click, the arrow keys.
  useEffect(() => {
    localStorage.setItem(PICKERS_W_KEY, String(pickersW));
  }, [pickersW]);

  // A width dragged out on a wide window would otherwise squeeze the pickers to nothing when
  // the window is made small again — re-clamp against the room there actually is.
  useEffect(() => {
    const el = row.current;
    if (!el) return;
    const ro = new ResizeObserver(() => widen(pickersW));
    ro.observe(el);
    return () => ro.disconnect();
  }, [widen, pickersW]);

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
    await reload();
    // Never fatal: a mods folder that can't be inspected for this is still a mods folder,
    // and the tab has to open either way.
    setRepairs(await scanGearRepairs().catch(() => []));
  }, [reload]);

  // Only the repairs on mount — the kit above this tab has already scanned the mods folder,
  // and doing it again here would read the whole thing twice every time the tab is opened.
  useEffect(() => {
    void scanGearRepairs()
      .then(setRepairs)
      .catch(() => setRepairs([]));
  }, []);

  // The bikes a swap on this bike lines up with — the ones sharing its .geom mount points.
  const hasSwaps = !!(bike && scans?.modelSwaps[bike]?.length);
  const [lineup, setLineup] = useState<string[] | null>(null);
  useEffect(() => {
    setLineup(null);
    if (!hasSwaps) return;
    let alive = true;
    void modelSwapLineup(bike)
      .then((b) => alive && setLineup(b))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [bike, hasSwaps]);

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

  // With no model picked, preview the game's own model rather than whatever swap happens to
  // be on the bike right now. A look composed here is meant to be shared, and "what's on my
  // disk" renders differently on everyone else's.
  //
  // Only when the scan actually offers Stock. A bike whose files are all packed has nothing
  // loose to park, so no Stock row is listed for it — and its active model already *is* the
  // packed one, which is what asking for Stock would have shown anyway.
  const offersStock = (scans?.modelSwaps[bike] ?? []).some(
    (v) => v.toLowerCase() === "stock",
  );
  const bikeVariant =
    loadout.modelSwap || (offersStock ? "Stock" : pickedModel(bike, loadout, scans));
  const lineupNames = lineup?.map((b) => b.replace(/^MX[0-9E]OEM_/i, "").replace(/_/g, " "));
  const lineupHint =
    lineupNames &&
    (lineupNames.length ? (
      <span className="line-clamp-3" title={lineupNames.join(", ")}>
        {t("rider.swapLinesUp", { bikes: lineupNames.join(", ") })}
      </span>
    ) : (
      t("rider.swapLinesUpNone")
    ));
  const showBike = bikePreview && !!bike;
  // A bike handed over by Presets comes from the profile's own list, which the target scan
  // should already cover — but if it doesn't, keep it pickable rather than showing an empty
  // trigger over a preview that is drawing that very bike.
  const bikeOptions = bike && !bikes.includes(bike) ? [bike, ...bikes] : bikes;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-none items-center gap-3.5 px-4 pb-3.5">
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
          className="mx-7 mb-3 flex items-start gap-2.5 rounded-lg border border-warning/40 bg-warning/10 px-3 py-2.5 text-[12.5px]"
        >
          <FolderInput className="mt-0.5 size-4 flex-none text-warning" />
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

      <div ref={row} className="flex min-h-0 flex-1 gap-5 overflow-hidden px-4 pb-4">
        {/* Picker column */}
        <section
          className="flex flex-none flex-col gap-4 overflow-y-auto pr-1"
          style={{ width: pickersW }}
        >
          {/* Show-on-model toggles */}
          <Card className="flex flex-wrap items-center gap-x-5 gap-y-2 p-3.5">
            <span className="text-[11px] font-semibold uppercase tracking-wide text-faint">
              {t("rider.showOnModel")}
            </span>
            {TOGGLES.map(({ part, label }) => (
              <label key={part} className="flex items-center gap-2 text-[12px] text-muted-foreground">
                <Switch checked={!hidden.includes(part)} onCheckedChange={() => toggleHidden(part)} />
                {t(label)}
              </label>
            ))}
          </Card>

          {/* Bike — the other half of a preset's look, and what the pair view draws */}
          {bikePreview && (
            <div className="flex flex-col gap-1.5">
              <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
                {t("slotGroup.bike")}
              </h2>
              <div className="grid grid-cols-1 gap-2">
                {/* Searchable, and matching the two slot fields beside it. A mods folder
                    runs to dozens of bikes, which is a long way to scroll for a name you
                    already know. Neither free text nor empty is a bike, so both are off. */}
                {/* Same row as every other slot — it is one, it just isn't a `SlotField`. */}
                <div className="flex flex-col border border-border bg-card px-3 py-1.5">
                  <span className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
                    {t("slotGroup.bike")}
                  </span>
                  <Combobox
                    value={bike}
                    options={bikeOptions}
                    onChange={setBike}
                    placeholder={t("slotGroup.bike")}
                    allowCreate={false}
                    allowEmpty={false}
                    className="h-6 border-0 bg-transparent px-0 text-[12.5px]"
                  />
                </div>
                {BIKE_SLOTS.map((slot) => (
                  <SlotField
                    row
                                        key={slot.key}
                    slot={slot}
                    value={loadout[slot.key]}
                    options={optionsFor(slot, bike, scans)}
                    missing={missingFor(slot, bike, scans)}
                    hint={slot.key === "modelSwap" ? lineupHint : undefined}
                    compact
                    onChange={(v) => setSlot(slot.key, v)}
                  />
                ))}
              </div>
            </div>
          )}

          {/* Rider slot groups */}
          {grouped.map((g) => (
            <div key={g.id} className="flex flex-col gap-1.5">
              <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
                {t(g.label)}
              </h2>
              <div className="grid grid-cols-1 gap-2">
                {g.slots.map((slot) => (
                  <SlotField
                    row
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

        {/* Live render — the rider, and the bike beside them when this build can draw one */}
        <div className="relative min-w-0 flex-1">
          {/* Drag the preview wider. It sits in the gap between the two columns rather than
              inside either, so neither loses a pixel to it. */}
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label={t("viewer.resizePanel")}
            title={t("viewer.resizePanel")}
            tabIndex={0}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
              drag.current = { from: e.clientX, was: pickersW };
            }}
            onPointerMove={onDragMove}
            onPointerUp={() => (drag.current = null)}
            onPointerCancel={() => (drag.current = null)}
            onDoubleClick={() => widen(PICKERS_W.initial)}
            onKeyDown={(e) => {
              const step = e.key === "ArrowLeft" ? -24 : e.key === "ArrowRight" ? 24 : 0;
              if (!step) return;
              e.preventDefault();
              widen(pickersW + step);
            }}
            className="group absolute -left-3.5 top-0 z-10 h-full w-3 cursor-col-resize touch-none focus:outline-none"
          >
            <span className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 rounded bg-foreground/[0.07] transition-colors group-hover:bg-primary/60 group-focus:bg-primary/60" />
          </div>
          <ViewerPanel
            loadout={loadout}
            riderOnly={!showBike}
            bikeId={showBike ? bike : undefined}
            bikeVariant={bikeVariant}
            hiddenParts={hidden}
            className="h-full"
          />
        </div>
      </div>
    </div>
  );
}
