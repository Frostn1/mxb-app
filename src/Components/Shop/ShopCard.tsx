import { useState } from "react";
import { ExternalLink, ShoppingBag, Store } from "lucide-react";
import type { ShopMod } from "../../types";
import PriceTag from "./PriceTag";
import { openShopUrl } from "../../api/shop";
import { cachedImage, GRID_THUMB_WIDTH } from "../../lib/imgcache";
import { useT } from "../../i18n/context";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
} from "@/Components/ui/context-menu";

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
          className="group relative flex cursor-default flex-col overflow-hidden rounded-xl border border-white/[0.07] bg-card text-left transition-colors hover:border-white/15"
        >
          <div className="relative aspect-square overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
            {mod.image && !broken ? (
              // The tile is square because the store's product images are, so `cover` fills
              // it edge to edge without cropping anything off the ~93% that are square. Only
              // the rare widescreen one loses a sliver at the sides, which beats letterboxing
              // every card to accommodate the exception.
              <img
                src={cachedImage(mod.image, GRID_THUMB_WIDTH)}
                alt={mod.title}
                loading="lazy"
                onError={() => setBroken(true)}
                className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
              />
            ) : (
              <div className="grid size-full place-items-center text-foreground/20">
                <Store className="size-8" strokeWidth={1.5} />
              </div>
            )}
            {mod.price.onSale && mod.price.discountPct !== null && (
              <span className="absolute right-2 top-2 rounded-md bg-emerald-500 px-1.5 py-[3px] text-[10.5px] font-bold text-black shadow-sm">
                −{mod.price.discountPct}%
              </span>
            )}
          </div>
          <div className="flex flex-col gap-1 px-3 py-2.5">
            <span
              className="truncate text-[13.5px] font-semibold"
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
