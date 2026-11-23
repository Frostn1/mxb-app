import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowRightToLine,
  Box,
  ChevronDown,
  ChevronRight,
  Eye,
  EyeOff,
  MoreHorizontal,
  RefreshCw,
  Scissors,
  Trash2,
  Undo2,
  Grid2x2,
} from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@frost/shared/Components/ui/context-menu";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { ROLES, type LibraryPart, type Role } from "../../../api/bikebuild";
import { nothingToSplit } from "./splitErrors";
import { Kbd, Tip } from "./Tip";
import type { useBikeLibrary } from "./useBikeLibrary";

/** What the outliner (and so the Properties panel) has selected: a part, an empty slot, or
 *  every part on the bike (A, as in Blender). */
export type Selection = { kind: "part"; id: string } | { kind: "slot"; role: Role } | { kind: "all" } | null;

/** A context-menu item's key, on the right as Blender's menus show it. */
function MenuKey({ keys }: { keys: string }) {
  return <Kbd className="ml-auto pl-1">{keys}</Kbd>;
}

/** Everything a part row can do, owned by `BikeBuilder` so the action bar, the outliner's
 *  menu and the Properties panel all run the very same thing. */
export interface PartActions {
  place: (p: LibraryPart) => void;
  takeOff: (p: LibraryPart) => void;
  split: (p: LibraryPart) => void;
  refresh: (p: LibraryPart) => void;
  useAsBase: (p: LibraryPart) => void;
  remove: (p: LibraryPart) => void;
}

/** A part has something to look at: shown as a small amber mark on its row. */
export function partWarnings(p: LibraryPart): boolean {
  return p.missing || p.stale || p.missingTextures.length > 0 || p.multiPartHint;
}

/**
 * The outliner, Blender-style: the bike at the top, each of its slots under it, and the part
 * in each slot under that, with an eye to hide it in the 3D view. Empty slots stay in the
 * tree, dimmed, so what's missing is plain to see. Parts in the library that aren't on the
 * bike sit in their own group underneath.
 *
 * Replaces the big part cards: everything they offered is a right-click (or the row's "…")
 * away — place, take off, split, refresh, use as base, remove — and the rest (slot,
 * location, rotation, tris, warnings) is in the Properties panel for whatever's selected.
 */
export default function Outliner({
  lib,
  baseName,
  selection,
  onSelect,
  hidden,
  onToggleHidden,
  actions,
  onStartDrag,
}: {
  lib: ReturnType<typeof useBikeLibrary>;
  baseName: string | null;
  selection: Selection;
  onSelect: (s: Selection) => void;
  /** Part ids hidden in the 3D view. A view setting only: nothing in the build changes. */
  hidden: Set<string>;
  onToggleHidden: (id: string) => void;
  actions: PartActions;
  /** A row with a role is also a drag handle onto the 3D view — see `usePartDrag`. */
  onStartDrag: (part: LibraryPart, e: React.PointerEvent) => void;
}) {
  const t = useT();
  const { parts, slots } = lib;
  const [bikeOpen, setBikeOpen] = useState(true);
  const [looseOpen, setLooseOpen] = useState(true);
  const byId = new Map((parts ?? []).map((p) => [p.id, p]));
  const onBike = new Set(Object.values(slots).filter((id): id is string => !!id));
  const loose = (parts ?? []).filter((p) => !onBike.has(p.id));

  return (
    <section
      data-dock="left"
      aria-label={t("bike.outliner")}
      className="flex h-full min-h-0 min-w-0 flex-col bg-window/40"
    >
      <header className="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.outliner")}</h2>
      </header>
      <div role="tree" className="min-h-0 flex-1 overflow-y-auto py-1 text-[12.5px]">
        <Row
          depth={0}
          open={bikeOpen}
          onToggle={() => setBikeOpen((o) => !o)}
          icon={<Grid2x2 className="size-3.5 text-primary" />}
          label={<span className="font-semibold">{baseName ?? t("bike.slots")}</span>}
          title={baseName ?? undefined}
        />
        {bikeOpen &&
          ROLES.map((role) => {
            const part = slots[role] ? byId.get(slots[role]!) : undefined;
            const slotSelected = selection?.kind === "slot" && selection.role === role;
            return (
              // A drop target for a part dragged from "Not on the bike" (see `usePartDrag`).
              <div key={role} role="group" data-bike-slot={role}>
                <Row
                  depth={1}
                  selected={slotSelected}
                  dim={!part}
                  onClick={() => onSelect(part ? { kind: "part", id: part.id } : { kind: "slot", role })}
                  title={part ? undefined : t("bike.slotTip", { fits: t(`bike.fits.${role}`) })}
                  label={
                    <span className={cn("text-[11.5px] uppercase tracking-[0.04em]", part ? "text-muted-foreground" : "text-faint")}>
                      {t(`bike.role.${role}`)}
                    </span>
                  }
                  trailing={
                    !part ? <span className="truncate text-[10.5px] italic text-faint">{t("bike.slotDropHint")}</span> : null
                  }
                />
                {part && (
                  <PartRow
                    part={part}
                    depth={2}
                    placed
                    selected={selection?.kind === "all" || (selection?.kind === "part" && selection.id === part.id)}
                    hidden={hidden.has(part.id)}
                    onToggleHidden={() => onToggleHidden(part.id)}
                    onSelect={() => onSelect({ kind: "part", id: part.id })}
                    actions={actions}
                    busy={lib.busy}
                  />
                )}
              </div>
            );
          })}
        {loose.length > 0 && (
          <>
            <div className="mx-3 my-1.5 h-px bg-border" />
            <Row
              depth={0}
              open={looseOpen}
              onToggle={() => setLooseOpen((o) => !o)}
              icon={<Box className="size-3.5 text-faint" />}
              label={<span className="font-semibold text-muted-foreground">{t("bike.notPlaced")}</span>}
              trailing={<span className="text-[11px] tabular-nums text-faint">{loose.length}</span>}
            />
            {looseOpen &&
              loose.map((p) => (
                <PartRow
                  key={p.id}
                  part={p}
                  depth={1}
                  placed={false}
                  selected={selection?.kind === "part" && selection.id === p.id}
                  onSelect={() => onSelect({ kind: "part", id: p.id })}
                  actions={actions}
                  busy={lib.busy}
                  onStartDrag={onStartDrag}
                />
              ))}
          </>
        )}
        {parts !== null && parts.length === 0 && (
          <p className="px-3 pt-3 text-[12px] text-muted-foreground">{t("bike.partsEmpty")}</p>
        )}
      </div>
    </section>
  );
}

/** One line of the tree. Indent by depth, an optional disclosure chevron, label, and a slot
 *  on the right for row icons. */
function Row({
  depth,
  open,
  onToggle,
  icon,
  label,
  trailing,
  selected,
  dim,
  onClick,
  title,
  onPointerDown,
}: {
  depth: number;
  open?: boolean;
  onToggle?: () => void;
  icon?: React.ReactNode;
  label: React.ReactNode;
  trailing?: React.ReactNode;
  selected?: boolean;
  dim?: boolean;
  onClick?: () => void;
  title?: string;
  onPointerDown?: (e: React.PointerEvent) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Picked in the 3D view: the row comes into sight here, as Blender's outliner does.
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);
  return (
    <div
      ref={ref}
      role="treeitem"
      aria-selected={selected}
      // The shared selected look (studio.css, `--sel-*`): no tint of our own here.
      data-selected={selected ? "true" : undefined}
      aria-expanded={onToggle ? open : undefined}
      title={title}
      onClick={onClick ?? onToggle}
      onPointerDown={onPointerDown}
      className={cn(
        "group flex h-7 cursor-default select-none items-center gap-1.5 pr-2",
        "hover:bg-foreground/[0.05]",
        dim && "opacity-60",
      )}
      style={{ paddingLeft: 4 + depth * 10 }}
    >
      {onToggle ? (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
          className="flex size-4 shrink-0 items-center justify-center text-faint hover:text-foreground"
        >
          {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
        </button>
      ) : (
        <span className="size-4 shrink-0" />
      )}
      {icon}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {trailing}
    </div>
  );
}

function PartRow({
  part,
  depth,
  placed,
  selected,
  hidden,
  onToggleHidden,
  onSelect,
  actions,
  busy,
  onStartDrag,
}: {
  part: LibraryPart;
  depth: number;
  placed: boolean;
  selected: boolean;
  hidden?: boolean;
  onToggleHidden?: () => void;
  onSelect: () => void;
  actions: PartActions;
  busy: boolean;
  onStartDrag?: (part: LibraryPart, e: React.PointerEvent) => void;
}) {
  const t = useT();
  const warn = partWarnings(part);
  const icon = "flex size-5 shrink-0 items-center justify-center rounded text-faint hover:bg-foreground/[0.08] hover:text-foreground";

  const menu = (
    <>
      {placed ? (
        <ContextMenuItem disabled={busy} onSelect={() => actions.takeOff(part)}>
          <Undo2 />
          {t("bike.takeOff")}
          <MenuKey keys="X" />
        </ContextMenuItem>
      ) : (
        <ContextMenuItem disabled={busy || !part.role} onSelect={() => actions.place(part)}>
          <ArrowRightToLine />
          {part.role ? t("bike.place") : t("bike.placeNeedsRole")}
          <MenuKey keys="Ctrl+P" />
        </ContextMenuItem>
      )}
      <ContextMenuItem
        disabled={busy || nothingToSplit(part)}
        title={nothingToSplit(part) ? t("bike.splitNothing") : undefined}
        onSelect={() => actions.split(part)}
      >
        <Scissors />
        {t("bike.splitIntoParts")}
        <MenuKey keys="P" />
      </ContextMenuItem>
      {part.role !== "chassis" || !placed ? (
        <ContextMenuItem disabled={busy} onSelect={() => actions.useAsBase(part)}>
          <Grid2x2 />
          {t("bike.useAsBase")}
        </ContextMenuItem>
      ) : null}
      {!part.missing && (
        <ContextMenuItem disabled={busy} onSelect={() => actions.refresh(part)}>
          <RefreshCw />
          {t("bike.refreshPart")}
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem variant="destructive" disabled={busy} onSelect={() => actions.remove(part)}>
        <Trash2 />
        {t("bike.removePart")}
      </ContextMenuItem>
    </>
  );

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div>
          <Row
            depth={depth}
            selected={selected}
            dim={placed && hidden}
            onClick={onSelect}
            title={`${part.name}
${part.source}${!placed && part.role ? `\n${t("bike.dragToPlace")}` : ""}`}
            onPointerDown={
              onStartDrag && part.role && !busy
                ? (e) => {
                    if (e.button === 0) onStartDrag(part, e);
                  }
                : undefined
            }
            icon={<Box className={cn("size-3.5 shrink-0", placed ? "text-primary/80" : "text-faint")} />}
            label={
              <span className="flex min-w-0 items-center gap-1.5">
                <span className="truncate">{part.name}</span>
                {!placed && part.role && (
                  <span className="shrink-0 text-[10.5px] text-faint">{t(`bike.role.${part.role}`)}</span>
                )}
              </span>
            }
            trailing={
              <span className="flex shrink-0 items-center gap-0.5">
                {warn && <AlertTriangle className="size-3 text-amber-500" />}
                <MoreButton title={t("bike.moreActions")} className={icon} />
                {placed && onToggleHidden && (
                  <Tip label={hidden ? t("bike.show") : t("bike.hide")} keys={hidden ? "Alt+H" : "H"} side="right">
                    <button
                      type="button"
                      aria-label={hidden ? t("bike.show") : t("bike.hide")}
                      onClick={(e) => {
                        e.stopPropagation();
                        onToggleHidden();
                      }}
                      className={icon}
                    >
                      {hidden ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                    </button>
                  </Tip>
                )}
                {!placed && part.role && (
                  <Tip label={t("bike.placeTip")} keys="Ctrl+P" side="right">
                    <button
                      type="button"
                      aria-label={t("bike.place")}
                      disabled={busy}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={(e) => {
                        e.stopPropagation();
                        actions.place(part);
                      }}
                      className={cn(icon, "disabled:opacity-40")}
                    >
                      <ArrowRightToLine className="size-3.5" />
                    </button>
                  </Tip>
                )}
              </span>
            }
          />
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>{menu}</ContextMenuContent>
    </ContextMenu>
  );
}

/** The row's "…": opens the same menu as a right-click, for anyone who doesn't think to
 *  right-click. Re-dispatches as a contextmenu event on the row, so it's one menu, not two. */
function MoreButton({ title, className }: { title: string; className: string }) {
  return (
    <button
      type="button"
      title={title}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        const r = e.currentTarget.getBoundingClientRect();
        e.currentTarget.dispatchEvent(
          new MouseEvent("contextmenu", { bubbles: true, clientX: r.left, clientY: r.bottom, button: 2 }),
        );
      }}
      className={cn(className, "hidden focus-visible:flex group-hover:flex")}
    >
      <MoreHorizontal className="size-3.5" />
    </button>
  );
}
