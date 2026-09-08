import { useState } from "react";
import { ExternalLink, ShoppingBag, Store } from "lucide-react";
import type { ShopMod } from "@frost/shared/types";
import PriceTag from "./PriceTag";
import { openShopUrl } from "../../api/shop";
import { GRID_THUMB_WIDTH } from "@frost/shared/lib/imgcache";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import { useT } from "@/i18n";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
} from "@frost/shared/Components/ui/context-menu";

interface ShopCardProps {
  mod: ShopMod;
  currency: string;
  onOpen: () => void;
}

/**
 * A catalog item in the grid.
 *
 * A sibling of `ModCard` rather than a variant of it. `ModCard` is built around installing:
 * a selection checkbox, a rating overlay, an "Installed" badge and a context menu whose
 * first action is Quick Install. A shop item has none of that and needs a price row and a
 * sale badge instead — so bolting five optional props onto a component shared by Browse, the
 * overlay and the downloads page would cost more than these ninety lines.
 *
 * The Tailwind classes are deliberately identical to `ModCard`'s, so the two grids look like
 * one product rather than two.
 */
export default function ShopCard({ mod, currency, onOpen }: ShopCardProps) {
  const t = useT();
  const [broken, setBroken] = useState(false);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          onClick={onOpen}
          className="group u-notch relative flex cursor-default flex-col overflow-hidden bg-card text-left transition-colors"
        >
          <div className="relative aspect-square overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
            {mod.image && !broken ? (
              // The tile is square because the store's product images are, so `cover` fills
              // it edge to edge without cropping anything off the ~93% that are square. Only
              // the rare widescreen one loses a sliver at the sides, which beats letterboxing
              // every card to accommodate the exception.
              <CachedImg
                src={mod.image}
                width={GRID_THUMB_WIDTH}
                alt={mod.title}
                loading="lazy"
                onUnavailable={() => setBroken(true)}
                className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
              />
            ) : (
              <div className="grid size-full place-items-center text-foreground/20">
                <Store className="size-8" strokeWidth={1.5} />
              </div>
            )}
            {mod.price.onSale && mod.price.discountPct !== null && (
              <span className="u-skew absolute right-2 top-2 bg-success px-2 py-[3px]">
                <span className="u-unskew block font-cond text-[11px] font-bold tracking-[0.06em] text-[#0d1216]">
                  −{mod.price.discountPct}%
                </span>
              </span>
            )}
          </div>
          <div className="flex flex-col gap-1 px-3 py-2.5">
            <span
              className="truncate font-cond text-[14px] font-bold uppercase tracking-[0.05em]"
              title={mod.title}
            >
              {mod.title}
            </span>
            <div className="flex items-baseline justify-between gap-2">
              <PriceTag price={mod.price} currency={currency} />
              {mod.author && (
                <span
                  className="truncate text-[11.5px] text-muted-foreground"
                  title={mod.author}
                >
                  {mod.author}
                </span>
              )}
            </div>
          </div>
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={onOpen}>
          <ShoppingBag className="size-4" /> {t("shopCatalog.viewDetails")}
        </ContextMenuItem>
        {mod.url && (
          <ContextMenuItem onSelect={() => void openShopUrl(mod.url)}>
            <ExternalLink className="size-4" /> {t("shopCatalog.openOnStore")}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
