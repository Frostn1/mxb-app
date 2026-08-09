import { useState } from "react";
import { Check, Download, ExternalLink, Loader2, Store } from "lucide-react";
import type { ShopMod } from "../../types";
import type { ShopItem } from "../../api/mods";
import { openShopUrl } from "../../api/shop";
import { cachedImage, GRID_THUMB_WIDTH } from "../../lib/imgcache";
import { useT } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/Components/ui/select";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
} from "@/Components/ui/context-menu";

/** One purchased product, with every file the store lists under it. */
export interface Purchase {
  /** The product's name — the grouping key, and what the catalog was matched on. */
  product: string;
  /** Every file sold under this product. Never empty; length > 1 means variants (PRO/AMS/…). */
  files: ShopItem[];
  /** The catalog entry for this product, when the store publishes one. */
  listing: ShopMod | null;
  /** Whether something by this name is already in the library. */
  installed: boolean;
}

interface PurchaseCardProps {
  purchase: Purchase;
  /** True while this card's file is downloading — the sheet takes over from there. */
  busy: boolean;
  /** Download completion, 0–1. Null when the server sent no Content-Length. */
  progress: number | null;
  /** Any card being staged disables the rest: one plan may be staged at a time. */
  disabled: boolean;
  onInstall: (file: ShopItem) => void;
}

/**
 * A purchased product in the grid.
 *
 * The third sibling of `ModCard` and `ShopCard`, for the same reason those two are separate:
 * this one is neither browsing nor buying. There is no price (it is already paid for) and no
 * rating, but there *is* a variant picker, which neither of the others has — a product can
 * ship a PRO and an AMS file under one name, and the store sells them as one thing.
 *
 * Tailwind classes stay identical to `ModCard`/`ShopCard` so all three grids read as one app.
 *
 * Artwork, author and store link come from the public catalog, matched by product name in
 * Rust (`shop_catalog::match_products`). A purchase the catalog doesn't list — delisted, or
 * simply named differently — keeps the placeholder rather than borrowing someone else's image.
 */
export default function PurchaseCard({
  purchase,
  busy,
  progress,
  disabled,
  onInstall,
}: PurchaseCardProps) {
  const t = useT();
  const [broken, setBroken] = useState(false);
  const { product, files, listing, installed } = purchase;

  // Which file to install. Only ever offered when the product ships more than one.
  const [pickedId, setPickedId] = useState(String(files[0].id));
  const picked = files.find((f) => String(f.id) === pickedId) ?? files[0];
  const multi = files.length > 1;

  const install = () => onInstall(picked);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="group relative flex flex-col overflow-hidden rounded-xl border border-white/[0.07] bg-card text-left transition-colors hover:border-white/15">
          <button
            onClick={install}
            disabled={disabled}
            className="relative aspect-square cursor-default overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a] disabled:cursor-not-allowed"
          >
            {listing?.image && !broken ? (
              <img
                src={cachedImage(listing.image, GRID_THUMB_WIDTH)}
                alt={product}
                loading="lazy"
                onError={() => setBroken(true)}
                className="size-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
              />
            ) : (
              <div className="grid size-full place-items-center text-foreground/20">
                <Store className="size-8" strokeWidth={1.5} />
              </div>
            )}

            {installed && (
              <span className="absolute left-2 top-2 flex items-center gap-1 rounded-md bg-emerald-500 px-1.5 py-[3px] text-[10.5px] font-bold text-black shadow-sm">
                <Check className="size-3" strokeWidth={3} />
                {t("purchases.installed")}
              </span>
            )}
            {multi && (
              <span className="absolute right-2 top-2 rounded-md bg-black/65 px-1.5 py-[3px] text-[10.5px] font-semibold text-white/90 shadow-sm backdrop-blur-sm">
                {t("purchases.fileCount", { count: files.length })}
              </span>
            )}

            {busy && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-background/70 backdrop-blur-sm">
                <Loader2 className="size-7 animate-spin text-primary" />
                {/* A purchased track runs to hundreds of megabytes; a bare spinner for that
                    long reads as a hang. No bar when the server withheld a length — an
                    invented one would be a lie. */}
                {progress !== null && (
                  <>
                    <div className="h-1 w-3/5 overflow-hidden rounded-full bg-foreground/15">
                      <div
                        className="h-full rounded-full bg-primary transition-[width] duration-200"
                        style={{ width: `${Math.round(progress * 100)}%` }}
                      />
                    </div>
                    <span className="text-[11px] font-medium tabular-nums text-muted-foreground">
                      {Math.round(progress * 100)}%
                    </span>
                  </>
                )}
              </div>
            )}
          </button>

          <div className="flex flex-col gap-1.5 px-3 py-2.5">
            <span className="truncate text-[13.5px] font-semibold" title={product}>
              {product}
            </span>
            {listing?.author && (
              <span
                className="truncate text-[11.5px] text-muted-foreground"
                title={listing.author}
              >
                {listing.author}
              </span>
            )}

            {/* Only a product with variants gets a picker; a single file has nothing to ask. */}
            {multi && (
              <Select value={pickedId} onValueChange={setPickedId} disabled={disabled}>
                <SelectTrigger className="h-7 w-full bg-background text-[11.5px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {files.map((f) => (
                    <SelectItem key={f.id} value={String(f.id)}>
                      {f.fileLabel || f.title}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}

            <Button
              size="sm"
              variant={installed ? "outline" : "default"}
              className="mt-0.5 h-7 w-full text-[12px]"
              onClick={install}
              disabled={disabled}
            >
              {busy ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Download className="size-3.5" />
              )}
              {busy
                ? t("purchases.downloading")
                : installed
                  ? t("purchases.reinstall")
                  : t("purchases.install")}
            </Button>
          </div>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={install} disabled={disabled}>
          <Download className="size-4" />
          {installed ? t("purchases.reinstall") : t("purchases.install")}
        </ContextMenuItem>
        {listing?.url && (
          <ContextMenuItem onSelect={() => void openShopUrl(listing.url)}>
            <ExternalLink className="size-4" /> {t("shopCatalog.openOnStore")}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
