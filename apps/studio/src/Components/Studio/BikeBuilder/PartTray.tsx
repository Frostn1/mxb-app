import { useState } from "react";
import { AlertTriangle, ArrowRightToLine, Box, Loader2, Plus, RefreshCw, Sparkles, Trash2 } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import { ROLES, type LibraryPart, type Role } from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";
import PartMaker from "./PartMaker";

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
 * The tray: every part brought into the library, one card each — the only place a part's
 * role is set, and (with "Place") the only place other than the viewport itself that puts one
 * on the bike. Bringing a part in and saying what it is used to happen here while a *second*
 * control, on the outliner, also picked which part filled a slot; that duplication is gone —
 * placing is drag it, click an open mount in the viewport, or "Place" right here.
 */
export default function PartTray({
  ready,
  lib,
  onStartDrag,
  onChanged,
}: {
  ready: boolean;
  lib: ReturnType<typeof useBikeLibrary>;
  /** Picked up the thumbnail as a drag handle — see `usePartDrag` for why this is a plain
   *  pointer sequence and not HTML5 drag-and-drop. */
  onStartDrag: (part: LibraryPart, e: React.PointerEvent) => void;
  /** For the "Make a part…" dialog's `PartMaker`, which joins the same library but isn't
   *  one of `lib`'s own mutations. */
  onChanged: () => void;
}) {
  const t = useT();
  const { parts, slots, adding, busy, onAdd, add, onRole, onRemove, onSplit, onPlace, onUseAsBase } = lib;
  const [making, setMaking] = useState(false);

  return (
    <section data-dock="left" className="flex h-full min-w-0 flex-col gap-3 overflow-y-auto p-4">
      <div className="flex items-center gap-2">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("bike.parts")}
        </h2>
        <div className="ml-auto flex gap-1.5">
          <Button size="sm" variant="outline" onClick={() => setMaking(true)} disabled={!ready || busy}>
            <Sparkles className="size-3.5" />
            {t("bike.makeAPart")}
          </Button>
          <Button size="sm" onClick={onAdd} disabled={!ready || busy}>
            {adding ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
            {adding ? t("bike.adding", { n: adding.n, of: adding.of }) : t("bike.addParts")}
          </Button>
        </div>
      </div>
      {parts === null ? null : parts.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("bike.partsEmpty")}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {parts.map((p) => {
            const placed = !!p.role && slots[p.role] === p.id;
            return (
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
                    <p className="pl-[18px] text-amber-600/80">
                      {t("bike.roleHintsFound")}{" "}
                      {(Object.entries(p.roleHints) as [Role, number][])
                        .map(([role, n]) => (n > 1 ? `${t(`bike.role.${role}`)} ×${n}` : t(`bike.role.${role}`)))
                        .join(" · ")}
                    </p>
                    <div className="flex gap-1.5">
                      <Button size="sm" onClick={() => onUseAsBase(p)} disabled={busy}>
                        {t("bike.useAsBase")}
                      </Button>
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
                  <Button
                    size="sm"
                    variant={placed ? "ghost" : "default"}
                    className="flex-1"
                    onClick={() => onPlace(p)}
                    disabled={busy || !p.role || placed}
                    title={!p.role ? t("bike.placeNeedsRole") : undefined}
                  >
                    <ArrowRightToLine className="size-3.5" />
                    {placed ? t("bike.placed") : t("bike.place")}
                  </Button>
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
                    title={t("bike.removePart")}
                    onClick={() => onRemove(p)}
                    disabled={busy}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <Dialog open={making} onOpenChange={setMaking}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>{t("bike.makeAPart")}</DialogTitle>
          </DialogHeader>
          <div className="max-h-[70vh] overflow-y-auto">
            <PartMaker ready={ready} onChanged={onChanged} />
          </div>
        </DialogContent>
      </Dialog>
    </section>
  );
}
