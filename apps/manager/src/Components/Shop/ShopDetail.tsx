import { useEffect, useState } from "react";
import {
  ChevronLeft,
  Check,
  Download,
  ExternalLink,
  FileBox,
  Loader2,
  ShoppingBag,
  Store,
  User,
} from "lucide-react";
import type { HubModDetail, ShopModDetail } from "@frost/shared/types";
import type { ShopItem } from "@frost/shared/api/mods";
import { formatPrice, openShopUrl, shopCatalogDetail } from "../../api/shop";
import PriceTag, { SaleEnds } from "./PriceTag";
import RichDescription from "../ModDetail/RichDescription";
import { Button } from "@frost/shared/Components/ui/button";
import { Skeleton } from "@frost/shared/Components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useI18n, useT } from "@/i18n";
import { formatDate } from "@frost/shared/lib/mods";
import { ContextBarLeft } from "../Shell/ContextBar";
import { ActionBar, StateChip } from "../ModPage/ActionBar";
import MediaPanel, { type Figure } from "../ModPage/Media";
import { Panel, SectionLabel, WhatsInside } from "../ModPage/Panels";

/** What the right rail offers when the viewer already owns this. */
export interface OwnedActions {
  /** Every file sold under this product; length > 1 means variants (PRO/AMS/…). */
  files: ShopItem[];
  installed: boolean;
  /** True while this product is downloading. */
  busy: boolean;
  /** Download completion, 0–1, or null when no length was reported. */
  progress: number | null;
  disabled: boolean;
  onInstall: (file: ShopItem) => void;
}

interface ShopDetailProps {
  id: number;
  currency: string;
  onBack: () => void;
  /** Turns this into the detail page for something already bought. */
  owned?: OwnedActions;
  /**
   * Where the detail comes from. Defaults to the mxbikes-shop catalog; MXB Hub passes its own
   * so both stores share this page rather than growing a second copy of it. Everything below
   * reads a `ShopModDetail` and does not care which store produced it.
   */
  load?: (id: number) => Promise<ShopModDetail>;
}

/**
 * One catalog item, on the one mod page.
 *
 * The same bar, picture and card column as Browse and the library — the state card here is
 * the price (or, once it is yours, the download). What this page deliberately does not show
 * is a file size: neither store states one anywhere in its catalog, and the only honest
 * numbers are the ones the transfer itself reports.
 */
export default function ShopDetail({
  id,
  currency,
  onBack,
  owned,
  load = shopCatalogDetail,
}: ShopDetailProps) {
  const t = useT();
  const { resolved } = useI18n();
  const [detail, setDetail] = useState<ShopModDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Which file to install, for a product that ships more than one. Held here rather than in
  // `owned` so the picker survives a re-render of the parent grid.
  const [pickedId, setPickedId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setError(null);
    load(id)
      .then((d) => !cancelled && setDetail(d))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [id, load]);

  const crumb = (title: string) => (
    <ContextBarLeft>
      <span className="flex items-center gap-2 font-cond text-[12.5px] font-semibold tracking-[-0.02em]">
        <button
          onClick={onBack}
          className="flex cursor-default items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
        >
          <ChevronLeft className="size-3.5" />
          {t("common.back")}
        </button>
        <span className="text-faint">/</span>
        <span className="max-w-[420px] truncate text-foreground">{title}</span>
      </span>
    </ContextBarLeft>
  );

  if (error) {
    return (
      <div className="flex min-h-0 flex-1 flex-col px-7 pt-5">
        {crumb("—")}
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
      <div className="flex min-h-0 flex-1 flex-col px-7 pt-5">
        {crumb("…")}
        <div className="mt-4 flex gap-6">
          <Skeleton className="aspect-video flex-1 rounded-xl" />
          <Skeleton className="h-64 w-[320px] flex-none rounded-xl" />
        </div>
      </div>
    );
  }

  // The Hub's own one-line statement of what ships ("in-game ready PKZ", "PSD included").
  // The mxbikes-shop dump carries no equivalent, so this is simply absent there.
  const summary = (detail as Partial<HubModDetail>).summary ?? null;

  const price = detail.price;
  const live = price.onSale ? price.sale : price.base;
  const priceLabel = price.free
    ? t("shopCatalog.free")
    : live === null
      ? null
      : formatPrice(live, currency, resolved);

  const files = owned?.files ?? [];
  const picked = files.find((f) => String(f.id) === pickedId) ?? files[0];

  const figures: Figure[] = [
    ...(priceLabel ? [{ label: "Price", value: priceLabel }] : []),
    ...(detail.updated !== null
      ? [
          {
            label: t("shopCatalog.updated"),
            value: formatDate(new Date(detail.updated * 1000).toISOString()),
          },
        ]
      : []),
  ];

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      {crumb(detail.title)}

      <ActionBar
        image={detail.image ?? detail.images[0] ?? null}
        fallbackIcon={ShoppingBag}
        title={detail.title}
        meta={[detail.author, detail.categoryNames[0]]}
      >
        {owned ? (
          <>
            {owned.installed && (
              <StateChip icon={Check} tone="success">
                {t("purchases.installed")}
              </StateChip>
            )}
            {picked && (
              <Button
                variant={owned.installed ? "secondary" : "default"}
                onClick={() => owned.onInstall(picked)}
                disabled={owned.disabled || owned.busy}
              >
                {owned.busy ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Download className="size-4" />
                )}
                {owned.busy
                  ? t("purchases.downloading")
                  : owned.installed
                    ? t("purchases.reinstall")
                    : t("modDetail.addToLibrary")}
              </Button>
            )}
          </>
        ) : (
          detail.url && (
            <Button onClick={() => void openShopUrl(detail.url)}>
              <ExternalLink className="size-4" />
              {priceLabel ? `${t("shopCatalog.buyOnStore")} · ${priceLabel}` : t("shopCatalog.buyOnStore")}
            </Button>
          )
        )}
      </ActionBar>

      <div className="flex min-h-0 flex-1 gap-6 px-7 pb-5 pt-4">
        <div className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
          {/* The store's product art is square; `contain` in a 16:9 frame keeps it whole
              rather than slicing its top and bottom off to fill. */}
          <MediaPanel
            images={detail.images}
            title={detail.title}
            figures={figures}
            emptyLabel={t("shopCatalog.noScreenshots")}
            fit="contain"
          />

          {detail.descriptionHtml && (
            <div className="flex flex-col gap-2">
              <SectionLabel>{t("shopCatalog.about")}</SectionLabel>
              {/* Authored HTML from the store's catalog. Sanitised in Rust before it ever
                  reaches here — see `sanitize_html` in `mods/shop_catalog.rs`. */}
              <RichDescription html={detail.descriptionHtml} />
            </div>
          )}
        </div>

        <div className="flex w-[320px] flex-none flex-col gap-3 overflow-y-auto pb-1">
          {owned ? (
            <OwnedPanel
              owned={owned}
              picked={picked}
              setPickedId={setPickedId}
              storeUrl={detail.url}
            />
          ) : (
            <Panel>
              <PriceTag price={detail.price} currency={currency} size="lg" />
              <SaleEnds price={detail.price} />
              {/* Buying happens on the store. We deliberately don't handle payment or
                  downloads — this app can browse the catalog and nothing more. */}
              {!detail.url && (
                <p className="text-[12px] text-muted-foreground">
                  {t("shopCatalog.noProductLink")}
                </p>
              )}
              <p className="text-[11px] text-faint">{t("shopCatalog.buyNote")}</p>
            </Panel>
          )}

          {/* What's inside, said before you own it wherever the store says it: the Hub's own
              summary line, and — once it's yours — the files the product actually ships. */}
          {summary && (
            <Panel label="What's inside">
              <p className="text-[12.5px] leading-relaxed text-muted-foreground">{summary}</p>
            </Panel>
          )}
          <WhatsInside
            groups={
              files.length > 0
                ? [
                    {
                      key: "files",
                      label: t("purchases.fileCount", { count: files.length }),
                      items: files.map((f) => ({
                        key: String(f.id),
                        label: f.fileLabel || f.title,
                        icon: FileBox,
                      })),
                    },
                  ]
                : []
            }
          />

          <Panel label={t("modDetail.details")}>
            {detail.author && (
              <div className="flex items-start justify-between gap-4 text-[12px]">
                <span className="flex flex-none items-center gap-1.5 text-muted-foreground">
                  <User className="size-3.5" />
                  {t("shopCatalog.author")}
                </span>
                {detail.authorUrl ? (
                  <button
                    onClick={() => void openShopUrl(detail.authorUrl)}
                    className="min-w-0 cursor-default truncate text-right text-primary hover:underline"
                  >
                    {detail.author}
                  </button>
                ) : (
                  <span className="min-w-0 truncate text-right text-foreground/85">
                    {detail.author}
                  </span>
                )}
              </div>
            )}
            {detail.categoryNames.length > 0 && (
              <div className="flex items-start justify-between gap-4 text-[12px]">
                <span className="flex flex-none items-center gap-1.5 text-muted-foreground">
                  <Store className="size-3.5" />
                  {t("shopCatalog.category")}
                </span>
                {/* Wraps rather than truncates: a dozen categories on one line would push
                    themselves out of the card instead of being clipped by it. */}
                <span className="min-w-0 whitespace-normal break-words text-right text-foreground/85">
                  {detail.categoryNames.join(", ")}
                </span>
              </div>
            )}
          </Panel>
        </div>
      </div>
    </div>
  );
}

/** The state card for something already bought: pick a file, watch it land. */
function OwnedPanel({
  owned,
  picked,
  setPickedId,
  storeUrl,
}: {
  owned: OwnedActions;
  picked: ShopItem | undefined;
  setPickedId: (id: string) => void;
  storeUrl: string | null;
}) {
  const t = useT();
  const { files, busy, progress, disabled } = owned;
  const multi = files.length > 1;

  return (
    <Panel label={t("purchases.install")}>
      {/* Only a product with variants has anything to ask. */}
      {multi && picked && (
        <Select value={String(picked.id)} onValueChange={setPickedId} disabled={disabled}>
          <SelectTrigger className="h-8 w-full bg-background text-[12.5px]">
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

      {/* No bar when nothing reported a length — an invented one would be a lie. */}
      {busy && progress !== null && (
        <div className="flex flex-col gap-1">
          <div className="h-1 w-full overflow-hidden rounded-full bg-foreground/15">
            <div
              className="h-full rounded-full bg-primary transition-[width] duration-200"
              style={{ width: `${Math.round(progress * 100)}%` }}
            />
          </div>
          <span className="text-center text-[11px] font-medium tabular-nums text-muted-foreground">
            {Math.round(progress * 100)}%
          </span>
        </div>
      )}

      {storeUrl && (
        <Button
          variant="ghost"
          size="sm"
          className="w-full"
          onClick={() => void openShopUrl(storeUrl)}
        >
          <ExternalLink className="size-3.5" />
          {t("shopCatalog.openOnStore")}
        </Button>
      )}
    </Panel>
  );
}
