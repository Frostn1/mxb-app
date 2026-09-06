import type { ShopPrice } from "../../types";
import { formatPrice } from "../../api/shop";
import { useI18n } from "../../i18n/context";
import { cn } from "@/lib/utils";

interface PriceTagProps {
  price: ShopPrice;
  currency: string;
  /** `lg` is the detail view's rail; the default suits a card. */
  size?: "sm" | "lg";
  className?: string;
}

/**
 * What an item costs, including the four shapes that combination produces:
 *
 *   $24.99                       plain
 *   $24.99 – $34.99              a range (several options, e.g. paint / paint + PSD)
 *   ~~$24.99~~ $19.99  −20%      on sale
 *   ~~$24.99 – $34.99~~ $19.99 – $27.99  −20%    a range that is also on sale
 *
 * The last one is the case that's easy to get wrong — both ends have to move together, or
 * the struck-through figure stops corresponding to the live one.
 */
export default function PriceTag({
  price,
  currency,
  size = "sm",
  className,
}: PriceTagProps) {
  const { resolved, t } = useI18n();
  const money = (v: number | null) => formatPrice(v, currency, resolved);

  const big = size === "lg";

  // The store's own flag, not a price of 0 — a pay-what-you-want item starts at 0 without
  // being free, so this can't be inferred from the number.
  if (price.free) {
    return (
      <span
        className={cn(
          "font-semibold text-emerald-400",
          big ? "text-[22px]" : "text-[13px]",
          className,
        )}
      >
        {t("shopCatalog.free")}
      </span>
    );
  }

  // Nothing priced at all. Better to say so than to show "$0.00", which reads as free.
  if (price.base === null && price.sale === null) {
    return (
      <span className={cn("text-[12px] text-muted-foreground", className)}>
        {t("shopCatalog.priceUnknown")}
      </span>
    );
  }

  const range = (low: number | null, high: number | null) =>
    high !== null && high !== low ? `${money(low)} – ${money(high)}` : money(low);

  const normal = range(price.base, price.hasRange ? price.baseMax : null);
  const live = price.onSale
    ? range(price.sale, price.hasRange ? price.saleMax : null)
    : null;

  return (
    <span className={cn("flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5", className)}>
      {live ? (
        <>
          <span
            className={cn(
              "text-muted-foreground line-through",
              big ? "text-[14px]" : "text-[11.5px]",
            )}
          >
            {normal}
          </span>
          <span
            className={cn(
              "font-semibold text-foreground",
              big ? "text-[22px]" : "text-[13px]",
            )}
          >
            {live}
          </span>
          {price.discountPct !== null && price.discountPct > 0 && (
            <span
              className={cn(
                "rounded-[5px] bg-emerald-500/15 px-1.5 py-[1px] font-semibold text-emerald-400",
                big ? "text-[12px]" : "text-[10.5px]",
              )}
            >
              −{price.discountPct}%
            </span>
          )}
        </>
      ) : (
        <span
          className={cn(
            "font-semibold text-foreground",
            big ? "text-[22px]" : "text-[13px]",
          )}
        >
          {normal}
        </span>
      )}
    </span>
  );
}

/**
 * "Sale ends in 3 days", but only when the catalog genuinely carries an end date.
 *
 * Rendered separately from the price so a missing window is simply nothing on screen — the
 * UI must never invent an expiry, since for most items we have no idea when the sale stops.
 */
export function SaleEnds({ price }: { price: ShopPrice }) {
  const t = useI18n().t;
  if (!price.onSale || price.saleEnds === null) return null;

  const seconds = price.saleEnds - Math.floor(Date.now() / 1000);
  if (seconds <= 0) return null;

  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor(seconds / 3_600);
  const label =
    days >= 1
      ? t("shopCatalog.saleEndsDays", { count: days })
      : hours >= 1
        ? t("shopCatalog.saleEndsHours", { count: hours })
        : t("shopCatalog.saleEndsSoon");

  return <span className="text-[11.5px] text-emerald-400/80">{label}</span>;
}
