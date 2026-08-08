import { useEffect, useState } from "react";
import { ArrowLeft, ExternalLink, Store, User } from "lucide-react";
import type { ShopModDetail } from "../../types";
import { openShopUrl, shopCatalogDetail } from "../../api/shop";
import PriceTag, { SaleEnds } from "./PriceTag";
import Gallery from "../ModDetail/Gallery";
import RichDescription from "../ModDetail/RichDescription";
import { Button } from "@/Components/ui/button";
import { Skeleton } from "@/Components/ui/skeleton";
import { useT } from "../../i18n/context";
import { formatDate } from "../../lib/mods";

interface ShopDetailProps {
  id: number;
  currency: string;
  onBack: () => void;
}

/**
 * One catalog item, in full.
 *
 * A separate component from `ModDetail` rather than a mode of it. `ModDetail` is 690 lines
 * and most of them are install machinery — destination resolution, mirror sorting, the
 * install dialog, the blocked-host flow, the five-stage progress chain — none of which
 * applies to something we never download. The two genuinely shared pieces are the gallery
 * (now `ModDetail/Gallery.tsx`) and the `.mod-description` styling, and both are reused here.
 *
 * Served entirely from the catalog already in memory, so this opens instantly and works with
 * the network down.
 */
export default function ShopDetail({ id, currency, onBack }: ShopDetailProps) {
  const t = useT();
  const [detail, setDetail] = useState<ShopModDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setError(null);
    shopCatalogDetail(id)
      .then((d) => !cancelled && setDetail(d))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (error) {
    return (
      <div className="flex h-full flex-col px-7 pt-5">
        <BackButton label={t("common.back")} onBack={onBack} />
        <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
          <p className="text-[13px] font-semibold text-destructive">
            {t("shopCatalog.loadFailed")}
          </p>
          <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
            {error.replace(/^Error:\s*/, "")}
          </p>
        </div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex h-full flex-col px-7 pt-5">
        <BackButton label={t("common.back")} onBack={onBack} />
        <div className="mt-4 flex gap-6">
          <Skeleton className="aspect-video flex-1 rounded-xl" />
          <Skeleton className="h-64 w-[340px] flex-none rounded-xl" />
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden px-7 pb-6 pt-5">
      <div className="flex flex-none items-center gap-3">
        <BackButton label={t("common.back")} onBack={onBack} />
        <h1 className="truncate text-[17px] font-bold tracking-[-0.2px]">
          {detail.title}
        </h1>
      </div>

      <div className="mt-4 flex min-h-0 flex-1 gap-6">
        <div className="flex min-w-0 flex-1 flex-col gap-3.5 overflow-y-auto pr-1">
          {/* Square, because the store's product images are. In a 16:9 frame they had to
              be either cropped or heavily letterboxed; matching the frame to the content
              means the image fills it and stays whole. */}
          <Gallery
            images={detail.images}
            title={detail.title}
            emptyLabel={t("shopCatalog.noScreenshots")}
            aspect="square"
          />

          {detail.descriptionHtml && (
            <div className="flex flex-col gap-2 pt-1">
              <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
                {t("shopCatalog.about")}
              </span>
              {/* Authored HTML from the store's catalog. Sanitised in Rust before it ever
                  reaches here — see `sanitize_html` in `mods/shop_catalog.rs`. */}
              <RichDescription html={detail.descriptionHtml} />
            </div>
          )}
        </div>

        {/* right rail */}
        <div className="flex w-[340px] flex-none flex-col gap-3 overflow-y-auto">
          <div className="flex flex-col gap-2 rounded-xl border border-white/[0.07] bg-card p-4">
            <PriceTag price={detail.price} currency={currency} size="lg" />
            <SaleEnds price={detail.price} />

            {/* Buying happens on the store. We deliberately don't handle payment or
                downloads — this app can browse the catalog and nothing more. */}
            {detail.url ? (
              <Button
                className="mt-2 w-full"
                onClick={() => void openShopUrl(detail.url)}
              >
                <ExternalLink className="size-4" />
                {t("shopCatalog.buyOnStore")}
              </Button>
            ) : (
              <p className="mt-2 text-[12px] text-muted-foreground">
                {t("shopCatalog.noProductLink")}
              </p>
            )}
            <p className="text-center text-[11px] text-faint">
              {t("shopCatalog.buyNote")}
            </p>
          </div>

          <div className="flex flex-col gap-2.5 rounded-xl border border-white/[0.07] bg-card p-4 text-[12.5px]">
            {detail.author && (
              <Row icon={<User className="size-3.5" />} label={t("shopCatalog.author")}>
                {detail.authorUrl ? (
                  <button
                    onClick={() => void openShopUrl(detail.authorUrl)}
                    className="cursor-default truncate text-left text-primary hover:underline"
                  >
                    {detail.author}
                  </button>
                ) : (
                  <span className="truncate">{detail.author}</span>
                )}
              </Row>
            )}
            {detail.categoryNames.length > 0 && (
              <Row icon={<Store className="size-3.5" />} label={t("shopCatalog.category")}>
                <span className="truncate">{detail.categoryNames.join(", ")}</span>
              </Row>
            )}
            {detail.updated !== null && (
              <Row label={t("shopCatalog.updated")}>
                <span>{formatDate(new Date(detail.updated * 1000).toISOString())}</span>
              </Row>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function BackButton({ label, onBack }: { label: string; onBack: () => void }) {
  return (
    <Button variant="ghost" size="sm" onClick={onBack} className="flex-none">
      <ArrowLeft className="size-4" />
      {label}
    </Button>
  );
}

function Row({
  icon,
  label,
  children,
}: {
  icon?: React.ReactNode;
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="flex flex-none items-center gap-1.5 text-muted-foreground">
        {icon}
        {label}
      </span>
      <span className="ml-auto min-w-0 text-right">{children}</span>
    </div>
  );
}
