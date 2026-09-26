import { AlertTriangle, Box, Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import { ROLES, type LibraryPart } from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";

function Thumb({ part }: { part: LibraryPart | undefined }) {
  return part?.thumb ? (
    <img
      src={part.thumb}
      alt=""
      className="aspect-square w-full bg-muted/40 object-contain"
      draggable={false}
    />
  ) : (
    <div className="flex aspect-square w-full items-center justify-center bg-muted/40 text-faint">
      <Box className="size-5" />
    </div>
  );
}

/**
 * The tray: every part brought into the library, one card each, with its role. The left
 * panel of the bike builder's three — tray, slots, preview — so a rider can see what they've
 * added without it competing with the slot grid or the 3D view for the same scroll column.
 */
export default function PartTray({
  ready,
  lib,
  onStartDrag,
}: {
  ready: boolean;
  lib: ReturnType<typeof useBikeLibrary>;
  /** Picked up the thumbnail as a drag handle — see `usePartDrag` for why this is a plain
   *  pointer sequence and not HTML5 drag-and-drop. */
  onStartDrag: (part: LibraryPart, e: React.PointerEvent) => void;
}) {
  const t = useT();
  const { parts, adding, busy, onAdd, add, onRole, onRemove, onSplit } = lib;

  return (
    <section data-dock="left" className="flex h-full min-w-0 flex-col gap-3 overflow-y-auto p-4">
      <div className="flex items-center gap-2">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("bike.parts")}
        </h2>
        <Button size="sm" className="ml-auto" onClick={onAdd} disabled={!ready || busy}>
          {adding ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
          {adding ? t("bike.adding", { n: adding.n, of: adding.of }) : t("bike.addParts")}
        </Button>
      </div>
      {parts === null ? null : parts.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("bike.partsEmpty")}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {parts.map((p) => (
            <li key={p.id} className="flex flex-col gap-1.5 border border-border bg-background p-2">
              <div className="flex gap-2">
                <div
                  className={cn("w-16 shrink-0", p.role && "cursor-grab active:cursor-grabbing")}
                  title={p.role ? t("bike.dragToPlace") : undefined}
                  onPointerDown={(e) => p.role && onStartDrag(p, e)}
                >
                  <Thumb part={p} />
                </div>
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <p className="truncate text-sm font-medium" title={p.source}>
                    {p.name}
                  </p>
                  <p className="truncate text-[11px] text-muted-foreground">
                    {t("bike.partStats", { tris: p.tris.toLocaleString(), empties: p.empties.length })}
                  </p>
                  {(p.missing || p.stale) && (
                    <p className="flex items-center gap-1 text-[11px] text-amber-500">
                      <AlertTriangle className="size-3" />
                      {p.missing ? t("bike.partMissing") : t("bike.partStale")}
                    </p>
                  )}
                </div>
              </div>
              {p.multiPartHint && (
                <div className="flex flex-col gap-1.5 border border-amber-500/30 bg-amber-500/10 p-1.5 text-[11px] text-amber-600">
                  <p className="flex items-start gap-1.5">
                    <AlertTriangle className="mt-px size-3 shrink-0" />
                    {t("bike.multiPartHint")}
                  </p>
                  <Button
                    size="sm"
                    variant="outline"
                    className="border-amber-500/40 text-amber-600 hover:text-amber-600"
                    onClick={() => onSplit(p)}
                    disabled={busy}
                  >
                    {t("bike.splitIntoParts")}
                  </Button>
                </div>
              )}
              <Select value={p.role ?? NONE} onValueChange={(v) => onRole(p, v)} disabled={busy}>
                <SelectTrigger className="h-8">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NONE}>{t("bike.noRole")}</SelectItem>
                  {ROLES.map((r) => (
                    <SelectItem key={r} value={r}>
                      {t(`bike.role.${r}`)}
                      {p.role === r && p.roleGuessed ? ` · ${t("bike.guessed")}` : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <div className="flex gap-1">
                {!p.missing && (
                  <Button
                    size="sm"
                    variant="ghost"
                    title={t("bike.refreshPart")}
                    onClick={() => add([p.source])}
                    disabled={!ready || busy}
                  >
                    <RefreshCw className="size-3.5" />
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  className="ml-auto"
                  title={t("bike.removePart")}
                  onClick={() => onRemove(p)}
                  disabled={busy}
                >
                  <Trash2 className="size-3.5" />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
