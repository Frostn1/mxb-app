import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronDown, FolderOpen } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "@/lib/utils";
import { Combobox } from "../ui/combobox";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import {
  bikePreviewAvailable,
  EMPTY_RIDER_TARGETS,
  paintStudioHints,
  scanBikeTargets,
  scanRiderTargets,
  type RiderTargets,
} from "../../api/mods";
import { useConfig } from "../../Context/Config";
import type { GameInfo, PaintDest } from "../../types";
import { useT, type TKey } from "../../i18n/context";

/**
 * Where a paint is going — shared by both halves of the Studio.
 *
 * Paint Studio and the Designer arrive at a `.pnt` from opposite ends (files off disk vs. a
 * canvas drawn here), but the question of where it lands is identical for both: what is being
 * painted, which model, and therefore which folder under `mods/`. Answering it twice would
 * mean two lists of paint kinds and two copies of `relFor`, and the day a new gear slot
 * appears one of them would be missed.
 *
 * It carries the 3D preview's answer too — the kind decides which mesh wears the result, so
 * the same pick that aims the save also aims the viewer.
 */

/**
 * One thing that can be painted, for the title that's active.
 *
 * Built from the game's own rider layout rather than listed here, because the two titles do not
 * agree on what a rider is made of: MX Bikes has helmets with goggles, separate boots and
 * protection, and gloves hanging off the profile; GP Bikes has visored helmets and bakes boots
 * and gloves into the rider model. A hard-coded list is one title's list, and offering GP Bikes
 * a "Boots" destination would write a `.pnt` into a folder its loader never opens.
 *
 * The id encodes where it came from so the destination can be rebuilt from it alone.
 */
export interface PaintKindDef {
  id: string;
  label: TKey;
  /** Folder under `mods/rider` whose models this paints, or null for a bike. */
  area: string | null;
  /** Sub-folder of the model that receives the paint — `paints`, `goggles`, `gloves`. */
  sub: string;
}

/** `mods/rider/riders`, both titles: MX Bikes' rider profiles, GP Bikes' rider models. */
const RIDERS = "riders";

/**
 * The label a gear folder is known by in the UI.
 *
 * Keyed by folder so both titles reuse the same translations, and so a slot a future title adds
 * shows its folder name rather than nothing at all.
 */
const AREA_LABEL: Record<string, TKey> = {
  helmets: "paints.kind.helmet",
  boots: "paints.kind.boots",
  protections: "paints.kind.protection",
  protection: "paints.kind.protection",
  [RIDERS]: "paints.kind.kit",
  gloves: "paints.kind.gloves",
  goggles: "paints.kind.goggles",
};

/**
 * Labels for the folders that hang off a rider profile, where the folder name alone would
 * collide with a gear area's.
 *
 * MX Bikes has goggles in two places and they are genuinely different destinations: a pair
 * bought with a helmet lives in `helmets/<helmet>/goggles`, and a pair that came with the rider
 * lives in `riders/<profile>/goggles`. Both are offered — two buttons reading "Goggles" would
 * be a coin toss over which folder a paint lands in.
 */
const EXTRA_LABEL: Record<string, TKey> = {
  goggles: "paints.kind.profileGoggles",
};

/** Which `RiderTargets` list holds the models of a gear folder. */
const AREA_TARGETS: Record<string, keyof RiderTargets> = {
  helmets: "helmets",
  boots: "boots",
  protections: "protection",
  protection: "protection",
  animations: "animations",
  [RIDERS]: "profiles",
};

/** Extensions the backend will read. TGA first: it's what paint templates are traded in. */
export const IMAGE_EXTS = ["tga", "png", "jpg", "jpeg", "bmp", "webp"];

/**
 * The gear slot a kind previews on, absent for the ones worn on the body itself.
 *
 * Keyed by gear folder, and only the three the rider rig has anchors for — see
 * `RiderComposite`. GP Bikes reaches none of this: its rig has no bindings, so the preview is
 * off there entirely.
 */
export function gearPartOf(kind: PaintKindDef): "helmet" | "boots" | "protection" | undefined {
  switch (kind.area) {
    case "helmets":
      return "helmet";
    case "boots":
      return "boots";
    case "protections":
    case "protection":
      return "protection";
    default:
      return undefined;
  }
}

export function isBikeKind(kind: PaintKindDef): boolean {
  return kind.area === null;
}

/** Where a kind's paints live under `mods/`, given the model or profile they're for. */
export function relFor(kind: PaintKindDef, model: string): string {
  if (kind.area === null) return `bikes/${model}/${kind.sub}`;
  return `rider/${kind.area}/${model}/${kind.sub}`;
}

/**
 * Everything the active title can have painted, in the order to offer it.
 *
 * Gear areas come from `installable_areas()` on the Rust side, so the legacy singular
 * `protection` folder — kept readable but marked un-installable — never appears here and a new
 * paint can no longer be written into it.
 */
export function kindsFor(game: GameInfo): PaintKindDef[] {
  const label = (folder: string): TKey => AREA_LABEL[folder] ?? (folder as TKey);
  const kinds: PaintKindDef[] = [
    { id: "bike", label: "paints.kind.bike", area: null, sub: "paints" },
  ];
  for (const area of game.riderAreas) {
    // `animations` are riding styles, not gear: models with nothing to paint.
    if (area.paints) {
      kinds.push({ id: `area:${area.folder}`, label: label(area.folder), area: area.folder, sub: "paints" });
    }
    if (area.goggles) {
      kinds.push({
        id: `goggles:${area.folder}`,
        label: "paints.kind.goggles",
        area: area.folder,
        sub: "goggles",
      });
    }
  }
  kinds.push({ id: "kit", label: label(RIDERS), area: RIDERS, sub: "paints" });
  for (const extra of game.riderProfileExtras) {
    kinds.push({
      id: `extra:${extra}`,
      label: EXTRA_LABEL[extra] ?? label(extra),
      area: RIDERS,
      sub: extra,
    });
  }
  return kinds;
}

export interface PaintDestState {
  kind: PaintKindDef;
  setKind: (k: PaintKindDef) => void;
  /** Everything the active title can have painted. */
  kinds: PaintKindDef[];
  model: string;
  setModel: (m: string) => void;
  /** Models (or rider profiles) the chosen kind can be painted for. */
  models: string[];
  /** A folder of the player's own, chosen instead of installing into the game. */
  folder: string | null;
  pickFolder: () => Promise<void>;
  clearFolder: () => void;
  /** Resolved destination, or null while nothing can be aimed at yet. */
  dest: PaintDest | null;
  /** The sheet names this model binds — from its own mesh and from the paints installed for it. */
  hints: string[];
  /** This build can decode bike geometry — false on public builds, which have no bike mesh. */
  bikePreview: boolean;
  modelLabel: TKey;
}

/**
 * The destination state, with `onChange` fired whenever the answer moves.
 *
 * Callers use that to drop anything they'd computed from the old destination — Paint Studio
 * forgets the file it just wrote, the Designer drops the model it was previewing on.
 */
export function usePaintDest(onChange?: () => void): PaintDestState {
  const { game } = useConfig();
  const kinds = useMemo(() => kindsFor(game), [game]);
  const [kindId, setKindId] = useState(kinds[0].id);
  // Looked up rather than stored, so switching titles re-resolves against the new list instead
  // of leaving one game's slot selected while another game's folders are on screen.
  const kind = kinds.find((k) => k.id === kindId) ?? kinds[0];
  const [model, setModelState] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [bikes, setBikes] = useState<string[]>([]);
  const [targets, setTargets] = useState<RiderTargets>(EMPTY_RIDER_TARGETS);
  const [bikePreview, setBikePreview] = useState(true);
  const [hints, setHints] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    void Promise.all([
      scanBikeTargets().catch(() => [] as string[]),
      scanRiderTargets().catch(() => EMPTY_RIDER_TARGETS),
      bikePreviewAvailable().catch(() => true),
    ]).then(([b, r, preview]) => {
      if (!alive) return;
      setBikes(b);
      setTargets(r);
      setBikePreview(preview);
    });
    return () => {
      alive = false;
    };
  }, []);

  const models = useMemo(() => {
    if (kind.area === null) return bikes;
    const list = targets[AREA_TARGETS[kind.area] ?? "profiles"];
    // The rider folder is the one place the game itself supplies models: MX Bikes ships
    // `default_mx`/`default_sm`, so a suit has somewhere to go on an empty mods folder. GP Bikes
    // ships none — every suit there is made for a downloaded model — and its list is empty,
    // which is the fact rather than an omission.
    if (kind.area !== RIDERS) return list;
    return [...new Set([...list, ...game.riderStockProfiles])].sort((a, b) =>
      a.toLowerCase().localeCompare(b.toLowerCase()),
    );
  }, [kind, bikes, targets, game.riderStockProfiles]);

  // Keep the picked model on the list the kind offers — switching from a helmet to a bike
  // must not leave a helmet's name sitting in a bike destination.
  useEffect(() => {
    setModelState((m) => (models.includes(m) ? m : models[0] ?? ""));
  }, [models]);

  const dest = useMemo<PaintDest | null>(
    () =>
      folder
        ? { kind: "folder", path: folder }
        : model
          ? { kind: "mods", rel: relFor(kind, model) }
          : null,
    [folder, kind, model],
  );

  // Only meaningful for a destination inside the game — a plain folder has nothing to learn from.
  useEffect(() => {
    if (folder || !model) {
      setHints([]);
      return;
    }
    let alive = true;
    paintStudioHints(relFor(kind, model))
      .then((h) => alive && setHints(h))
      .catch(() => alive && setHints([]));
    return () => {
      alive = false;
    };
  }, [kind, model, folder]);

  const setKind = useCallback(
    (k: PaintKindDef) => {
      setKindId(k.id);
      onChange?.();
    },
    [onChange],
  );

  const setModel = useCallback(
    (m: string) => {
      setModelState(m);
      setFolder(null);
      onChange?.();
    },
    [onChange],
  );

  const pickFolder = useCallback(async () => {
    const picked = await openDialog({ directory: true });
    const dir = Array.isArray(picked) ? picked[0] : picked;
    if (dir) {
      setFolder(dir);
      onChange?.();
    }
  }, [onChange]);

  const clearFolder = useCallback(() => {
    setFolder(null);
    onChange?.();
  }, [onChange]);

  return {
    kind,
    setKind,
    kinds,
    model,
    setModel,
    models,
    folder,
    pickFolder,
    clearFolder,
    dest,
    hints,
    bikePreview,
    // A rider folder holds profiles (MX Bikes) or rider models (GP Bikes); everything else
    // holds models. Naming it right is the difference between "For" reading as a question
    // about a helmet and about the rider wearing it.
    modelLabel: kind.area === RIDERS ? "paints.profile" : "paints.model",
  };
}

/**
 * The destination as one line, opening the full picker only when asked.
 *
 * Where a paint goes is decided once and then not thought about again, while the canvas beside
 * it is looked at continuously — so the picker earns a button, not a column. Everything it used
 * to show is still one click away, in the same card, unchanged.
 */
export function PaintDestBar({ state, className }: { state: PaintDestState; className?: string }) {
  const t = useT();
  const { kind, model, folder } = state;
  const where = folder
    ? folder.replace(/\\/g, "/").split("/").slice(-2).join("/")
    : model
      ? relFor(kind, model)
      : t("paints.needTarget");
  return (
    <Popover>
      <PopoverTrigger
        className={cn(
          "flex min-w-0 items-center gap-2 rounded-md border border-input bg-card px-2.5 py-1.5 text-left transition-colors hover:border-ring",
          className,
        )}
      >
        <span className="flex-none text-[11.5px] font-semibold">{t(kind.label)}</span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground" title={where}>
          {where}
        </span>
        <ChevronDown className="size-3.5 flex-none text-muted-foreground" />
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[320px] p-0">
        <PaintDestCard state={state} bare />
      </PopoverContent>
    </Popover>
  );
}

/** The "Where it goes" card: kind, model, and the folder escape hatch. */
export function PaintDestCard({ state, bare }: { state: PaintDestState; bare?: boolean }) {
  const t = useT();
  const { kind, setKind, kinds, model, setModel, models, folder, pickFolder, clearFolder } = state;
  return (
    <div className={cn("p-3.5", !bare && "rounded-lg border border-border bg-card/40")}>
      <h2 className="mb-2.5 text-[13px] font-semibold">{t("paints.whereTitle")}</h2>

      <div className="flex flex-wrap gap-1.5">
        {kinds.map((k) => (
          <button
            key={k.id}
            type="button"
            onClick={() => setKind(k)}
            className={cn(
              "rounded-md border px-2 py-1 text-[11.5px] font-medium transition-colors",
              kind.id === k.id
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border text-muted-foreground hover:text-foreground",
            )}
          >
            {t(k.label)}
          </button>
        ))}
      </div>

      <div className="mt-3 flex flex-col gap-1">
        <span className="text-[11px] font-medium text-muted-foreground">
          {t(state.modelLabel)}
        </span>
        <Combobox value={model} options={models} onChange={setModel} />
        {!models.length && (
          <span className="text-[11px] leading-snug text-faint">{t("paints.noModels")}</span>
        )}
      </div>

      <div className="mt-3 flex flex-col gap-1.5">
        {folder ? (
          <div className="flex items-start gap-2 rounded-md border border-border bg-background/60 px-2 py-1.5">
            <FolderOpen className="mt-0.5 size-3.5 flex-none text-muted-foreground" />
            <span className="min-w-0 flex-1 break-all text-[11px] leading-snug">{folder}</span>
            <button
              type="button"
              className="text-[11px] text-muted-foreground hover:text-foreground"
              onClick={clearFolder}
            >
              {t("common.clear")}
            </button>
          </div>
        ) : (
          <p className="text-[11px] leading-snug text-faint">
            {t("paints.destPath", { rel: model ? relFor(kind, model) : "…" })}
          </p>
        )}
        <button
          type="button"
          className="self-start text-[11px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
          onClick={() => void pickFolder()}
        >
          {t("paints.saveElsewhere")}
        </button>
      </div>
    </div>
  );
}
