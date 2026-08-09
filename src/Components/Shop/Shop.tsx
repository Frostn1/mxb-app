import { useEffect, useState } from "react";
import { shopCatalogAvailable } from "../../api/shop";
import ShopCatalog from "./ShopCatalog";
import MyDownloads from "./MyDownloads";
import HelpHint from "@/Components/ui/help-hint";
import { cn } from "@/lib/utils";
import { useT } from "../../i18n/context";

type ShopTab = "catalog" | "purchases";

interface ShopProps {
  /** Bumped after any install, so the purchases grid re-scans its "Installed" badges. */
  refreshKey: number;
}

/**
 * The Shop view: the store's public catalog, and the things this account already bought.
 *
 * One view with two tabs rather than two sidebar entries — they are the same store, and the
 * natural path is browse, buy on the site, come back and install.
 *
 * The two halves are gated differently, which is why the strip is conditional. Browsing the
 * catalog needs a credential baked in at build time (`shop_credentials.rs`), and a build
 * without one is supported; purchases need only the user's own login, which any build can do.
 * So a credential-less build opens straight to purchases with no strip at all, rather than
 * offering a tab that cannot work.
 */
export default function Shop({ refreshKey }: ShopProps) {
  const t = useT();
  const [catalogAvailable, setCatalogAvailable] = useState<boolean | null>(null);
  const [tab, setTab] = useState<ShopTab>("catalog");

  // A compile-time fact on the Rust side, so it's asked once and can't change under us.
  useEffect(() => {
    let cancelled = false;
    shopCatalogAvailable()
      .then((ok) => {
        if (cancelled) return;
        setCatalogAvailable(ok);
        if (!ok) setTab("purchases");
      })
      .catch(() => !cancelled && setCatalogAvailable(false));
    return () => {
      cancelled = true;
    };
  }, []);

  // Nothing is rendered until the gate is known: mounting the catalog and swapping it out a
  // tick later makes it fetch for a tab the user was never going to see.
  if (catalogAvailable === null) return null;

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">{t("nav.shop")}</h1>
        <HelpHint title={t("nav.shop")} description={t("shop.help")} />

        {catalogAvailable && (
          <div className="flex items-center gap-0.5 rounded-lg border border-input bg-card p-0.5">
            <TabButton
              label={t("shopTab.catalog")}
              on={tab === "catalog"}
              onClick={() => setTab("catalog")}
            />
            <TabButton
              label={t("shopTab.purchases")}
              on={tab === "purchases"}
              onClick={() => setTab("purchases")}
            />
          </div>
        )}
      </header>

      {/* Both stay mounted: switching tabs must not re-fetch the catalog or drop a sign-in,
          and each half is cheap to keep in the tree once it has loaded. */}
      <div className={cn("min-h-0 flex-1 flex-col", tab === "catalog" ? "flex" : "hidden")}>
        {catalogAvailable && <ShopCatalog />}
      </div>
      <div className={cn("min-h-0 flex-1 flex-col", tab === "purchases" ? "flex" : "hidden")}>
        <MyDownloads refreshKey={refreshKey} />
      </div>
    </div>
  );
}

function TabButton({
  label,
  on,
  onClick,
}: {
  label: string;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "cursor-default rounded-md px-3 py-1 text-[12px] font-medium transition-colors",
        on
          ? "bg-foreground font-semibold text-background"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
    </button>
  );
}
