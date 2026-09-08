import { useCallback, useEffect, useState } from "react";
import type { GearPaints, Loadout, RiderPart } from "../types";
import { listInstalledGearPaints } from "../api/mods";
import { slotOptions, type Scans, type SlotDef } from "./presets";

const EMPTY: GearPaints = {
  paints: [],
  goggles: [],
  hasStock: false,
  hasStockGoggles: false,
};

/** The gear slots whose paints live with the model rather than loose on disk. */
type GearPart = Extract<RiderPart["part"], "helmet" | "boots" | "protection">;

/**
 * Every paint the chosen helmet, boots and protection can wear, merged with what the
 * library scan found loose on disk.
 *
 * Two sources, because neither sees the other. The library scan walks folders, so it finds
 * a paint pack installed loose — but never one packed inside a `<model>.pkz`, nor one the
 * game itself ships. `list_installed_gear_paints` asks the model, which answers for those.
 * A picker fed by only one of them shows one set and silently drops the other, which is
 * what the Presets tab did until it grew this.
 *
 * Shared rather than written twice: the Rider tab merged them and Presets didn't, and one
 * tab offering paints the other hides is exactly the drift worth designing out.
 */
export function useGearPaints(loadout: Loadout) {
  const [paints, setPaints] = useState<Record<GearPart, GearPaints>>({
    helmet: EMPTY,
    boots: EMPTY,
    protection: EMPTY,
  });

  const { helmet: helmetModel, boots: bootsModel, protection: protectionModel } = loadout;
  useEffect(() => {
    let alive = true;
    const grab = (part: GearPart, model: string) =>
      listInstalledGearPaints(part, model).catch(() => EMPTY);
    Promise.all([
      grab("helmet", helmetModel),
      grab("boots", bootsModel),
      grab("protection", protectionModel),
    ]).then(([helmet, boots, protection]) => {
      if (alive) setPaints({ helmet, boots, protection });
    });
    return () => {
      alive = false;
    };
  }, [helmetModel, bootsModel, protectionModel]);

  /** What the models themselves offer for a slot — nothing, for a slot they don't dress. */
  const packedFor = useCallback(
    (key: keyof Loadout): string[] => {
      switch (key) {
        case "helmetPaint":
          return paints.helmet.paints;
        case "gogglesPaint":
          return paints.helmet.goggles;
        case "bootsPaint":
          return paints.boots.paints;
        case "protectionPaint":
          return paints.protection.paints;
        default:
          return [];
      }
    },
    [paints],
  );

  /** A slot's full option list: what's installed loose, plus what the model carries. */
  const optionsFor = useCallback(
    (slot: SlotDef, bikeid: string, scans: Scans | null): string[] => {
      const base = scans ? slotOptions(slot, bikeid, loadout, scans) : [];
      return [...new Set([...base, ...packedFor(slot.key)])];
    },
    [loadout, packedFor],
  );

  /**
   * Whether the slot names something none of those sources can supply.
   *
   * Never before the scan has landed: with nothing yet to check against, every filled slot
   * would read as a mod you haven't installed, and the warning would flash on every mount.
   */
  const missingFor = useCallback(
    (slot: SlotDef, bikeid: string, scans: Scans | null): boolean => {
      const val = loadout[slot.key];
      if (!scans || slot.freeText || !val) return false;
      return !optionsFor(slot, bikeid, scans).includes(val);
    },
    [loadout, optionsFor],
  );

  return { optionsFor, missingFor };
}
