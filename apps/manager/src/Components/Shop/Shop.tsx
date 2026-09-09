import { useEffect, useState } from "react";
import { shopCatalogAvailable } from "../../api/shop";
import ShopCatalog from "./ShopCatalog";
import MyDownloads from "./MyDownloads";
import { ContextBarLeft, ContextBarRight, ContextTab } from "../Shell/ContextBar";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@frost/shared/i18n/context";

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
      {/* The store's own two halves sit beside the rail's Shop/Hub tabs. */}
      {catalogAvailable && (
      <ContextBarLeft>
        <ContextTab active={tab === "catalog"} onSelect={() => setTab("catalog")}>
          {t("shopTab.catalog")}
        </ContextTab>
        <ContextTab active={tab === "purchases"} onSelect={() => setTab("purchases")}>
          {t("shopTab.purchases")}
        </ContextTab>
      </ContextBarLeft>
      )}
      <ContextBarRight>
        <HelpHint title={t("nav.shop")} description={t("shop.help")} />
      </ContextBarRight>


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

