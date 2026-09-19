import { useState } from "react";
import {
  Check,
  Download,
  ExternalLink,
  Info,
  Mountain,
  ShoppingBag,
  Store,
} from "lucide-react";
import { GRID_THUMB_WIDTH } from "@frost/shared/lib/imgcache";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
} from "@frost/shared/Components/ui/context-menu";
import { formatDate } from "@frost/shared/lib/mods";
import { useT } from "@/i18n";
import { openShopUrl } from "../../api/shop";
import PriceTag from "../Shop/PriceTag";
import { SOURCE_LABELS, type MergedMod } from "./sources";

interface ModsCardProps {
  item: MergedMod;
  currency: string;
  /** mxb-mods only: already on disk. */
  installed?: boolean;
  onOpen: () => void;
  /** mxb-mods only — the stores have nothing to queue until you own the thing. */
  onQuickInstall?: () => void;
}

/**
 * One listing in the merged grid, whichever catalogue it came from.
 *
 * The card says **where** a mod is from, who made it and when it last changed, and then what
 * it costs — a price for a paid item, the word "Free" for everything else. What it
 * deliberately never says is how big the download is: no catalogue publishes one. A size is
 * known only while a transfer is running, and afterwards from the download history and the
 * library ledger, so a number here would either be invented or blank on most cards.
 *
 * `ModCard` stays the card for browsing mxb-mods on its own, where selecting a dozen mods and
 * queueing them in one go is the point. This one is for the grid that mixes sources.
 */
export default function ModsCard({
  item,
  currency,
  installed,
  onOpen,
  onQuickInstall,
}: ModsCardProps) {
  const t = useT();
  const [broken, setBroken] = useState(false);

  const isStore = item.source !== "mods";
  const Icon = isStore ? Store : Mountain;
  const sale = item.price?.onSale ? item.price.discountPct : null;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          onClick={onOpen}
          className="group relative flex cursor-default flex-col overflow-hidden rounded-xl bg-card text-left transition-colors"
        >
          <div className="relative aspect-[4/3] overflow-hidden bg-gradient-to-br from-[#2a2a30] to-[#131316]">
            {item.image && !broken ? (
              <CachedImg
                src={item.image}
                width={GRID_THUMB_WIDTH}
                alt={item.title}
                loading="lazy"
                onUnavailable={() => setBroken(true)}
                className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.04]"
              />
            ) : (
              <div className="grid size-full place-items-center text-foreground/15">
                <Icon className="size-8" strokeWidth={1.3} />
              </div>
            )}

            {/* Which catalogue this came from. The whole point of merging the grids is that
                you stop having to remember which tab you were on, so every card says it. */}
            <span className="absolute left-2 top-2 rounded-full bg-[rgba(8,8,10,0.78)] px-2 py-[3px]">
              <span className="block font-cond text-[9.5px] font-bold uppercase tracking-[0.12em] text-white/85">
                {t(SOURCE_LABELS[item.source])}
              </span>
            </span>

            {installed ? (
              <span className="absolute right-2 top-2 flex items-center gap-1 rounded-full bg-[rgba(8,8,10,0.82)] px-2 py-[3px] text-success">
                <Check className="size-3" strokeWidth={3} />
                <span className="font-cond text-[9px] font-bold uppercase tracking-[0.14em]">
                  {t("common.installed")}
                </span>
              </span>
            ) : sale !== null ? (
              <span className="absolute right-2 top-2 rounded-full bg-success px-2.5 py-[3px]">
                <span className="block font-cond text-[11px] font-bold tracking-[-0.02em] text-[#0d1216]">
                  −{sale}%
                </span>
              </span>
            ) : null}
          </div>

          <div className="flex flex-col gap-1 px-3 py-2.5">
            <span
              className="truncate font-cond text-[14px] font-bold tracking-[-0.02em]"
              title={item.title}
            >
              {item.title}
            </span>
            <div className="flex items-baseline gap-1.5 text-[11px] text-muted-foreground">
              <span className="flex-none tabular-figures">{updatedLabel(item.updated)}</span>
              {item.author && (
                <>
                  <span className="text-faint">/</span>
                  <span className="truncate" title={item.author}>
                    {item.author}
                  </span>
                </>
              )}
            </div>
            <div className="flex items-baseline justify-between gap-2">
              {item.price ? (
                <PriceTag price={item.price} currency={currency} />
              ) : (
                // mxb-mods is free, always. Saying so on the card is what makes a price on the
                // card beside it mean something.
                <span className="text-[13px] font-semibold text-emerald-400">
                  {t("shopCatalog.free")}
                </span>
              )}
            </div>
          </div>
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        {onQuickInstall && (
          <ContextMenuItem onSelect={onQuickInstall}>
            <Download className="size-4" />{" "}
            {installed ? t("browse.quickReinstall") : t("browse.quickInstall")}
          </ContextMenuItem>
        )}
        <ContextMenuItem onSelect={onOpen}>
          {isStore ? <ShoppingBag className="size-4" /> : <Info className="size-4" />}{" "}
          {isStore ? t("shopCatalog.viewDetails") : t("browse.openDetails")}
        </ContextMenuItem>
        {isStore && item.url && (
          <ContextMenuItem onSelect={() => void openShopUrl(item.url)}>
            <ExternalLink className="size-4" /> {t("shopCatalog.openOnStore")}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}

/** "Jul 8, 2026" from the unix seconds the stores hand back, in the app's language rather
 *  than the OS's. Blank where the catalogue said nothing. */
function updatedLabel(updated: number | null): string {
  if (updated === null) return "";
  const d = new Date(updated * 1000);
  if (Number.isNaN(d.getTime())) return "";
  return formatDate(d.toISOString());
}
