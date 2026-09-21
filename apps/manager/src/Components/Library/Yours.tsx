/**
 * The two Library lists that aren't about files: what you own, and what you want.
 *
 * Both are grids of the same card as the installed ones — same border, same 76×48 tile, same
 * two lines of text — because the answer to "do I have this" should look the same wherever it
 * is asked. Neither acts on a file: owning something is the store's fact and wanting something
 * is yours, so the only actions here are opening the place the thing lives and forgetting it.
 */
import { Check, Heart, Store, Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import { useT } from "@/i18n";
import SmartImg from "../ModPage/Img";
import type { OwnedRow } from "../../lib/useOwned";
import type { WishItem } from "../../lib/useWishlist";
import type { StoreId } from "../../api/shop";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";

function Card({
  image,
  title,
  subtitle,
  badge,
  onOpen,
  actions,
}: {
  image?: string | null;
  title: string;
  subtitle: string;
  badge?: ReactNode;
  onOpen?: () => void;
  actions?: ReactNode;
}) {
  return (
    <div
      role={onOpen ? "button" : undefined}
      tabIndex={onOpen ? 0 : undefined}
      onClick={onOpen}
      onKeyDown={(e) => e.key === "Enter" && onOpen?.()}
      className={cn(
        "group flex items-center gap-3 self-start rounded-xl border border-white/[0.07] bg-card p-3 transition-colors",
        onOpen && "cursor-pointer hover:border-white/15",
      )}
    >
      <div className="relative grid h-12 w-[76px] flex-none place-items-center overflow-hidden rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/25">
        {image ? (
          <SmartImg src={image} width={160} alt="" className="h-full w-full object-cover" />
        ) : (
          <Store className="size-5" strokeWidth={1.5} />
        )}
        {badge}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-[13px] font-semibold" title={title}>
          {title}
        </span>
        <span className="truncate text-[11px] text-muted-foreground">{subtitle}</span>
      </div>
      {actions}
    </div>
  );
}

/** Everything the player has bought, both stores in one grid. */
export function OwnedGrid({
  rows,
  loading,
  signedIn,
  error,
  onOpenStore,
}: {
  rows: OwnedRow[];
  loading: boolean;
  signedIn: boolean;
  error: string | null;
  onOpenStore?: (store: StoreId) => void;
}) {
  const t = useT();

  if (loading)
    return <p className="py-16 text-center text-[13px] text-muted-foreground">{t("owned.loading")}</p>;
  if (error)
    return <p className="select-text py-16 text-center text-[13px] text-destructive">{error}</p>;
  if (!signedIn)
    return (
      <div className="flex flex-col items-center gap-3 py-16 text-center">
        <p className="text-[13px] text-muted-foreground">{t("owned.signedOut")}</p>
        {onOpenStore && (
          <Button variant="outline" size="sm" onClick={() => onOpenStore("shop")}>
            <Store className="size-3.5" /> {t("owned.openStores")}
          </Button>
        )}
      </div>
    );
  if (rows.length === 0)
    return <p className="py-16 text-center text-[13px] text-muted-foreground">{t("owned.empty")}</p>;

  return (
    <div className="grid grid-cols-3 gap-3">
      {rows.map((row) => (
        <Card
          key={row.key}
          image={row.image}
          title={row.product}
          subtitle={[
            row.store === "shop" ? t("owned.fromShop") : t("owned.fromHub"),
            row.author,
            row.installed ? t("purchases.installed") : t("owned.notInstalled"),
          ]
            .filter(Boolean)
            .join(" · ")}
          badge={
            row.installed ? (
              <span className="absolute bottom-0.5 right-0.5 rounded bg-success/70 p-0.5 text-white">
                <Check className="size-3" strokeWidth={3} />
              </span>
            ) : undefined
          }
          onOpen={onOpenStore ? () => onOpenStore(row.store) : undefined}
        />
      ))}
    </div>
  );
}

/** What the player marked as wanted, from a mod page. */
export function WishlistGrid({
  items,
  onRemove,
  onOpenMod,
  onOpenStore,
}: {
  items: WishItem[];
  onRemove: (id: string) => void;
  /** Browse's own pages open in place; a store entry goes to its store. */
  onOpenMod?: (slug: string) => void;
  onOpenStore?: (store: StoreId) => void;
}) {
  const t = useT();

  if (items.length === 0)
    return (
      <div className="flex flex-col items-center gap-1.5 py-16 text-center">
        <Heart className="size-5 text-faint" />
        <p className="text-[13px] text-muted-foreground">{t("wishlist.empty")}</p>
        <p className="text-[12px] text-faint">{t("wishlist.emptyHint")}</p>
      </div>
    );

  return (
    <div className="grid grid-cols-3 gap-3">
      {items.map((w) => {
        const open =
          w.source === "browse"
            ? onOpenMod && (() => onOpenMod(w.slug))
            : onOpenStore && (() => onOpenStore(w.source as StoreId));
        return (
          <Card
            key={w.id}
            image={w.image}
            title={w.title}
            subtitle={w.author ?? t("wishlist.wanted")}
            onOpen={open || undefined}
            actions={
              <button
                title={t("wishlist.remove")}
                aria-label={t("wishlist.remove")}
                onClick={(e) => {
                  e.stopPropagation();
                  onRemove(w.id);
                }}
                className="flex-none cursor-default rounded-md p-1 text-faint opacity-0 transition-colors hover:bg-foreground/[0.06] hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
              >
                <Trash2 className="size-3.5" />
              </button>
            }
          />
        );
      })}
    </div>
  );
}
