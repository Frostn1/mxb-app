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
import type { ModRating, ModSummary } from "@frost/shared/types";
import RatingStars from "./RatingStars";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@frost/shared/Components/ui/context-menu";
import { cn } from "@frost/shared/lib/utils";
import { formatDateShort } from "@frost/shared/lib/mods";
import { GRID_THUMB_WIDTH } from "@frost/shared/lib/imgcache";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import { useT } from "@/i18n";

interface ModCardProps {
  mod: ModSummary;
  /** The site's score, when it has one. Undefined until the ratings request lands. */
  rating?: ModRating;
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
  rating,
  installed,
  isBike,
  selected,
  selectionActive,
  onOpen,
  onToggleSelect,
  onQuickInstall,
}: ModCardProps) {
  const t = useT();
  const [broken, setBroken] = useState(false);
  const Icon = isBike ? Bike : Mountain;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          onClick={onOpen}
          className={cn(
            "group u-notch relative flex h-[178px] cursor-default flex-col overflow-hidden bg-card text-left transition-all",
            selected && "outline outline-2 -outline-offset-2 outline-primary",
          )}
        >
          <div className="absolute inset-0 bg-gradient-to-br from-[#2a2a30] to-[#131316]">
            {mod.image && !broken ? (
              <CachedImg
                // Through the on-disk cache, so scrolling the grid twice doesn't re-fetch
                // every thumbnail; a miss falls back to the origin URL, and only then to the
                // placeholder below.
                src={mod.image}
                width={GRID_THUMB_WIDTH}
                alt={mod.title}
                loading="lazy"
                onUnavailable={() => setBroken(true)}
                className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.04]"
              />
            ) : (
              <div className="grid size-full place-items-center text-foreground/15">
                <Icon className="size-9" strokeWidth={1.2} />
              </div>
            )}
          </div>

          {/* The art carries the title, so it needs a floor dark enough to read on. */}
          <div className="absolute inset-x-0 bottom-0 h-3/5 bg-gradient-to-t from-[rgba(6,6,7,0.95)] via-[rgba(6,6,7,0.55)] to-transparent" />

          <span
            role="checkbox"
            aria-checked={selected}
            onClick={(e) => {
              e.stopPropagation();
              onToggleSelect();
            }}
            className={cn(
              "absolute left-2 top-2 grid size-5 cursor-default place-items-center border transition-opacity",
              selected
                ? "border-primary bg-primary text-primary-foreground opacity-100"
                : "border-white/50 bg-black/45 text-transparent hover:text-white/70",
              selected || selectionActive ? "opacity-100" : "opacity-0 group-hover:opacity-100",
            )}
          >
            <Check className="size-3.5" strokeWidth={3} />
          </span>

          {installed && (
            <span className="u-skew absolute right-2 top-2 flex items-center gap-1 bg-[rgba(8,8,10,0.82)] px-1.5 py-[3px]">
              <span className="u-unskew flex items-center gap-1 text-success">
                <Check className="size-3" strokeWidth={3} />
                <span className="font-cond text-[9px] font-bold uppercase tracking-[0.16em]">
                  {t("common.installed")}
                </span>
              </span>
            </span>
          )}

          <div className="absolute inset-x-0 bottom-0 flex flex-col gap-1 px-2.5 pb-2.5">
            {/* Unrated mods get nothing at all — five empty stars would read as a bad
                score rather than as "nobody has voted yet". */}
            {rating && rating.count > 0 && (
              <span className="mb-0.5 w-fit bg-black/55 px-1.5 py-[2px]">
                <RatingStars rating={rating} />
              </span>
            )}
            <span
              className="truncate font-cond text-[15px] font-bold uppercase leading-[1.1] tracking-[0.05em] text-white"
              title={mod.title}
            >
              {mod.title}
            </span>
            <div className="flex items-baseline gap-1.5 text-[11px] text-white/65">
              <span className="flex-none tabular-figures">{formatDateShort(mod.date)}</span>
              {mod.author && (
                <>
                  <span className="text-white/30">/</span>
                  <span className="truncate" title={mod.author}>
                    {mod.author}
                  </span>
                </>
              )}
            </div>
          </div>
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={onQuickInstall}>
          <Download className="size-4" />{" "}
          {installed ? t("browse.quickReinstall") : t("browse.quickInstall")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={onOpen}>
          <Info className="size-4" /> {t("browse.openDetails")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={onToggleSelect}>
          {selected ? (
            <>
              <SquareCheck className="size-4" /> {t("common.deselect")}
            </>
          ) : (
            <>
              <Square className="size-4" /> {t("common.select")}
            </>
          )}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
