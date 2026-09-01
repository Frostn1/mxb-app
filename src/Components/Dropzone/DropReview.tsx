import { useMemo, useState } from "react";
import {
  AlertTriangle,
  Bike as BikeIcon,
  Boxes,
  Check,
  ChevronsUpDown,
  CircleDot,
  FileQuestion,
  Layers,
  Loader2,
  Map as MapIcon,
  Palette,
  Shirt,
  Sparkles,
  Volume2,
} from "lucide-react";
import { Dialog, DialogContent, DialogTitle } from "@/Components/ui/dialog";
import { Button } from "@/Components/ui/button";
import { Badge } from "@/Components/ui/badge";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/Components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/Components/ui/popover";
import { cn } from "@/lib/utils";
import { useT } from "../../i18n/context";
import type { TKey } from "../../i18n";
import type { DropChoice, DropItem, DropKind, DropPlan, DropReason } from "../../types";

/** A row plus the edits the user has made to it. */
export interface RowState {
  item: DropItem;
  /** Unchecked rows are left out of the commit. */
  keep: boolean;
  subpath: string;
  destFolder: string;
  /** The organisational folder the user picked, before the structural one is appended. */
  picked: string;
  /** Label of the chosen destination, for display. A category root has an empty
   *  `destFolder`, so the folder string alone can't say whether a choice was made. */
  destLabel: string;
  fileCount: number;
  collisions: string[];
  /** A destination change is being re-costed on the backend. */
  busy: boolean;
}

const KIND_LABEL: Record<DropKind, TKey> = {
  modsTree: "drop.kind.modsTree",
  track: "drop.kind.track",
  bike: "drop.kind.bike",
  bikePaint: "drop.kind.bikePaint",
  soundSet: "drop.kind.soundSet",
  riderGear: "drop.kind.riderGear",
  tyres: "drop.kind.tyres",
  reshadePreset: "drop.kind.reshadePreset",
  unknown: "drop.kind.unknown",
};

const REASON_LABEL: Record<DropReason, TKey> = {
  modsTree: "drop.reason.modsTree",
  categoryDirs: "drop.reason.categoryDirs",
  paintsBundle: "drop.reason.paintsBundle",
  soundMarkers: "drop.reason.soundMarkers",
  trackMarkers: "drop.reason.trackMarkers",
  trackPackage: "drop.reason.trackPackage",
  bikeConfig: "drop.reason.bikeConfig",
  loosePaint: "drop.reason.loosePaint",
  gearFolders: "drop.reason.gearFolders",
  riderTexture: "drop.reason.riderTexture",
  gearTexture: "drop.reason.gearTexture",
  reshadePreset: "drop.reason.reshadePreset",
  packLayout: "drop.reason.packLayout",
  unrecognised: "drop.reason.unrecognised",
};

const KIND_ICON: Record<DropKind, typeof BikeIcon> = {
  modsTree: Boxes,
  track: MapIcon,
  bike: BikeIcon,
  bikePaint: Palette,
  soundSet: Volume2,
  riderGear: Shirt,
  tyres: CircleDot,
  reshadePreset: Sparkles,
  unknown: FileQuestion,
};

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v >= 10 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/**
 * Destination picker. The shared `Combobox` takes plain strings, but a destination has a
 * folder path as its value and a human bike name as its label, so this drives the same
 * Popover + Command primitives directly rather than bending that component out of shape.
 */
function DestinationPicker({
  value,
  label,
  choices,
  invalid,
  onChange,
}: {
  value: string;
  label: string;
  choices: DropChoice[];
  invalid: boolean;
  onChange: (v: string) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  // Creatable, like the destination picker in the install dialog: the offered list can't
  // cover a folder that doesn't exist yet, and a picker you can't add to is a dead end.
  const q = query.trim();
  const canCreate =
    q.length > 0 && !choices.some((c) => c.value.toLowerCase() === q.toLowerCase());

  const commit = (v: string) => {
    onChange(v);
    setQuery("");
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          role="combobox"
          aria-expanded={open}
          className={cn(
            "flex h-7 min-w-0 max-w-[280px] cursor-default items-center gap-1.5 rounded-md border px-2 text-[11.5px] transition-colors",
            invalid
              ? "border-warning/50 text-warning"
              : "border-white/[0.09] text-foreground hover:bg-foreground/[0.04]",
          )}
        >
          <span className="truncate">
            {label || t("drop.chooseDestination")}
          </span>
          <ChevronsUpDown className="size-3 flex-none opacity-50" />
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-[320px] p-0" align="start">
        <Command shouldFilter>
          <CommandInput
            placeholder={t("drop.searchDestinations")}
            value={query}
            onValueChange={setQuery}
          />
          <CommandList>
            <CommandEmpty>{t("drop.noDestinations")}</CommandEmpty>
            {canCreate && (
              <CommandGroup>
                <CommandItem value={`__use__${q}`} onSelect={() => commit(q)}>
                  <Check className="mr-2 size-3.5 opacity-0" />
                  <span className="truncate">
                    {t("combobox.use", { value: q })}
                  </span>
                </CommandItem>
              </CommandGroup>
            )}
            <CommandGroup>
              {choices.map((c) => (
                <CommandItem
                  key={`${c.subpath}:${c.value}:${c.label}`}
                  value={`${c.label} ${c.value}`}
                  onSelect={() => commit(c.value)}
                >
                  <Check
                    className={cn(
                      "mr-2 size-3.5",
                      c.value === value ? "opacity-100" : "opacity-0",
                    )}
                  />
                  <span className="truncate">{c.label}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

function Row({
  row,
  onToggle,
  onPick,
}: {
  row: RowState;
  onToggle: () => void;
  onPick: (v: string) => void;
}) {
  const t = useT();
  const [showCollisions, setShowCollisions] = useState(false);
  const { item } = row;
  const Icon = KIND_ICON[item.kind];
  const unresolved = item.needsChoice && !row.subpath;

  // `modsTree` keeps its own internal layout, so there is no single folder to name.
  const destLabel =
    item.kind === "modsTree"
      ? t("drop.destAsPackaged")
      : [row.subpath, row.destFolder].filter(Boolean).join("/");

  return (
    <div
      className={cn(
        "rounded-lg border px-3.5 py-2.5 transition-colors",
        row.keep
          ? "border-white/[0.07] bg-card"
          : "border-white/[0.04] bg-card/40 opacity-55",
      )}
    >
      <div className="flex items-start gap-3">
        <button
          onClick={onToggle}
          aria-label={row.keep ? t("drop.exclude") : t("drop.include")}
          className={cn(
            "mt-0.5 flex size-4 flex-none cursor-default items-center justify-center rounded border transition-colors",
            row.keep
              ? "border-primary bg-primary text-primary-foreground"
              : "border-white/20",
          )}
        >
          {row.keep && <Check className="size-3" />}
        </button>

        <Icon className="mt-0.5 size-4 flex-none text-muted-foreground" />

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[13px] font-semibold">{item.name}</span>
            <Badge variant={unresolved ? "warning" : "count"}>
              {t(KIND_LABEL[item.kind])}
            </Badge>
          </div>

          <div className="mt-0.5 text-[11.5px] text-muted-foreground">
            {item.detail ? `${item.detail} · ` : ""}
            {t(REASON_LABEL[item.reason])}
            {row.fileCount > 0 && (
              <>
                {" · "}
                {t("drop.fileCount", { count: row.fileCount })}
              </>
            )}
            {item.bytes > 0 && ` · ${formatBytes(item.bytes)}`}
          </div>

          <div className="mt-1.5 flex flex-wrap items-center gap-2">
            {item.choices.length > 0 ? (
              <>
                <DestinationPicker
                  value={row.picked}
                  label={row.destLabel}
                  choices={item.choices}
                  invalid={unresolved}
                  onChange={onPick}
                />
                {!unresolved && (
                  <span className="truncate font-mono text-[11px] text-muted-foreground">
                    {destLabel}
                  </span>
                )}
              </>
            ) : (
              <span className="truncate font-mono text-[11px] text-muted-foreground">
                {destLabel}
              </span>
            )}
            {row.busy && (
              <Loader2 className="size-3 animate-spin text-muted-foreground" />
            )}
          </div>

          {unresolved && (
            <div className="mt-1.5 flex items-center gap-1.5 text-[11px] text-warning">
              <AlertTriangle className="size-3 flex-none" />
              {t("drop.pickDestinationFirst")}
            </div>
          )}

          {row.collisions.length > 0 && (
            <div className="mt-1.5">
              <button
                onClick={() => setShowCollisions((v) => !v)}
                className="flex items-center gap-1.5 text-[11px] text-warning hover:underline"
              >
                <AlertTriangle className="size-3 flex-none" />
                {t("drop.replaces", { count: row.collisions.length })}
              </button>
              {showCollisions && (
                <ul className="mt-1 max-h-28 space-y-0.5 overflow-y-auto pl-[18px]">
                  {row.collisions.map((c) => (
                    <li
                      key={c}
                      className="truncate font-mono text-[10.5px] text-muted-foreground"
                    >
                      {c}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default function DropReview({
  plan,
  rows,
  installing,
  onToggle,
  onToggleAll,
  onPick,
  onCancel,
  onInstall,
}: {
  plan: DropPlan;
  rows: RowState[];
  installing: boolean;
  onToggle: (id: string) => void;
  onToggleAll: (keep: boolean) => void;
  onPick: (id: string, value: string) => void;
  onCancel: () => void;
  onInstall: () => void;
}) {
  const t = useT();

  const ready = useMemo(
    () => rows.filter((r) => r.keep && !(r.item.needsChoice && !r.subpath)),
    [rows],
  );
  const blocked = useMemo(
    () => rows.filter((r) => r.keep && r.item.needsChoice && !r.subpath),
    [rows],
  );
  const collisionCount = useMemo(
    () => ready.reduce((n, r) => n + r.collisions.length, 0),
    [ready],
  );
  const allKept = useMemo(() => rows.every((r) => r.keep), [rows]);

  return (
    <Dialog open>
      {/* Deliberately no `onOpenChange`, and both dismiss gestures are blocked: a stray click
          on the backdrop used to discard the whole staged plan — files already extracted, the
          classification already done — with no way back. Leaving is an explicit Cancel. */}
      <DialogContent
        className="flex max-h-[80vh] w-[640px] max-w-[92vw] flex-col gap-0 p-0"
        onInteractOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => e.preventDefault()}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <div className="flex items-center justify-between border-b border-white/[0.06] px-5 py-3.5">
          <div>
            <DialogTitle className="text-[15px] font-bold">
              {t("drop.found", { count: plan.items.length })}
            </DialogTitle>
            <p className="mt-0.5 text-[11.5px] text-muted-foreground">
              {t("drop.reviewHint")}
            </p>
          </div>
          {/* A split pack runs to dozens of rows, and picking four of fifty-five is a lot of
              clicking from a sheet that starts with everything checked. Hidden on the short
              lists, where it would only be one more thing to read. */}
          {rows.length > 3 && (
            <Button
              variant="ghost"
              size="sm"
              className="flex-none text-[11.5px]"
              disabled={installing}
              onClick={() => onToggleAll(!allKept)}
            >
              {allKept ? t("drop.selectNone") : t("drop.selectAll")}
            </Button>
          )}
        </div>

        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto px-5 py-4">
          {rows.map((row) => (
            <Row
              key={row.item.id}
              row={row}
              onToggle={() => onToggle(row.item.id)}
              onPick={(v) => onPick(row.item.id, v)}
            />
          ))}

          {plan.skipped.length > 0 && (
            <div className="mt-3 rounded-lg border border-white/[0.05] bg-card/40 px-3.5 py-2.5">
              <div className="flex items-center gap-2 text-[11.5px] font-semibold text-muted-foreground">
                <Layers className="size-3.5" />
                {t("drop.skipped", { count: plan.skipped.length })}
              </div>
              <ul className="mt-1 space-y-0.5">
                {plan.skipped.map((s) => (
                  <li key={s.name} className="text-[11px] text-muted-foreground">
                    <span className="font-mono">{s.name}</span> — {s.reason}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-white/[0.06] px-5 py-3.5">
          <div className="min-w-0 text-[11.5px] text-muted-foreground">
            {blocked.length > 0 ? (
              <span className="text-warning">
                {t("drop.needChoice", { count: blocked.length })}
              </span>
            ) : collisionCount > 0 ? (
              <span className="text-warning">
                {t("drop.willReplace", { count: collisionCount })}
              </span>
            ) : (
              t("drop.nothingOverwritten")
            )}
          </div>
          <div className="flex flex-none gap-2">
            <Button variant="ghost" onClick={onCancel} disabled={installing}>
              {t("common.cancel")}
            </Button>
            <Button
              onClick={onInstall}
              disabled={installing || ready.length === 0}
            >
              {installing && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
              {t("drop.install", { count: ready.length })}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
