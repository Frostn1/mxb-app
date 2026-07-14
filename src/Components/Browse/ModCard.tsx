import { useState } from "react";
import {
  Mountain,
  Bike,
  Check,
  Download,
  Info,
  SquareCheck,
  Square,
} from "lucide-react";
import type { ModSummary } from "../../types";
import { Badge } from "@/Components/ui/badge";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/Components/ui/context-menu";
import { cn } from "@/lib/utils";
import { formatDateShort } from "../../lib/mods";

interface ModCardProps {
  mod: ModSummary;
  installed: boolean;
  isBike: boolean;
  selected: boolean;
  /** True when any card is selected — keeps every checkbox visible. */
  selectionActive: boolean;
  onOpen: () => void;
  onToggleSelect: () => void;
  onQuickInstall: () => void;
}

export default function ModCard({
  mod,
  installed,
  isBike,
  selected,
  selectionActive,
  onOpen,
  onToggleSelect,
  onQuickInstall,
}: ModCardProps) {
  const [broken, setBroken] = useState(false);
  const Icon = isBike ? Bike : Mountain;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          onClick={onOpen}
          className={cn(
            "group relative flex cursor-default flex-col overflow-hidden rounded-xl border bg-card text-left transition-colors",
            selected
              ? "border-primary ring-1 ring-primary"
              : "border-white/[0.07] hover:border-white/15",
          )}
        >
          <div className="relative aspect-video overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
            {mod.image && !broken ? (
              <img
                src={mod.image}
                alt={mod.title}
                loading="lazy"
                onError={() => setBroken(true)}
                className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
              />
            ) : (
              <div className="grid size-full place-items-center text-foreground/20">
                <Icon className="size-8" strokeWidth={1.5} />
              </div>
            )}
            <span
              role="checkbox"
              aria-checked={selected}
              onClick={(e) => {
                e.stopPropagation();
                onToggleSelect();
              }}
              className={cn(
                "absolute left-2 top-2 grid size-5 cursor-default place-items-center rounded-[6px] border shadow-sm transition-opacity",
                selected
                  ? "border-primary bg-primary text-primary-foreground opacity-100"
                  : "border-white/50 bg-black/40 text-transparent hover:text-white/70",
                selected || selectionActive
                  ? "opacity-100"
                  : "opacity-0 group-hover:opacity-100",
              )}
            >
              <Check className="size-3.5" strokeWidth={3} />
            </span>
          </div>
          <div className="flex flex-col gap-1 px-3 py-2.5">
            <div className="flex items-center gap-1.5">
              <span
                className="flex-1 truncate text-[13.5px] font-semibold"
                title={mod.title}
              >
                {mod.title}
              </span>
              {installed && (
                <Badge variant="success" className="flex-none">
                  <Check className="size-3" strokeWidth={3} />
                  Installed
                </Badge>
              )}
            </div>
            <span className="text-[11.5px] text-muted-foreground">
              {formatDateShort(mod.date)}
            </span>
          </div>
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={onQuickInstall}>
          <Download className="size-4" /> {installed ? "Quick reinstall" : "Quick install"}
        </ContextMenuItem>
        <ContextMenuItem onSelect={onOpen}>
          <Info className="size-4" /> Open details
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onToggleSelect}>
          {selected ? (
            <>
              <SquareCheck className="size-4" /> Deselect
            </>
          ) : (
            <>
              <Square className="size-4" /> Select
            </>
          )}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
