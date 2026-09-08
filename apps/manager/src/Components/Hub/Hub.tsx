import { useState } from "react";
import HubCatalog from "./HubCatalog";
import HubPurchases from "./HubPurchases";
import { ContextBarLeft, ContextBarRight, ContextTab } from "../Shell/ContextBar";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@frost/shared/i18n/context";

type HubTab = "catalog" | "purchases";

interface HubProps {
  /** Bumped after any install, so the purchases grid re-scans its "Installed" badges. */
  refreshKey: number;
}

/**
 * MXB Hub: the store's public catalog, and the things this account already bought.
 *
 * One view with two tabs rather than two sidebar entries — they are the same store, and the
 * natural path is browse, buy on the site, come back and install. The same shape as `Shop`,
 * minus its conditional strip: both halves of this one work in every build, because browsing
 * MXB Hub needs no credential.
 */
export default function Hub({ refreshKey }: HubProps) {
  const t = useT();
  const [tab, setTab] = useState<HubTab>("catalog");

  return (
    <div className="flex h-full flex-col">
      <ContextBarLeft>
        <ContextTab active={tab === "catalog"} onSelect={() => setTab("catalog")}>
          {t("shopTab.catalog")}
        </ContextTab>
        <ContextTab active={tab === "purchases"} onSelect={() => setTab("purchases")}>
          {t("shopTab.purchases")}
        </ContextTab>
      </ContextBarLeft>
      <ContextBarRight>
        <HelpHint title={t("nav.hub")} description={t("hub.help")} />
      </ContextBarRight>


      {/* Both stay mounted: switching tabs must not re-fetch the catalog or drop a sign-in. */}
      <div className={cn("min-h-0 flex-1 flex-col", tab === "catalog" ? "flex" : "hidden")}>
        <HubCatalog />
      </div>
      <div className={cn("min-h-0 flex-1 flex-col", tab === "purchases" ? "flex" : "hidden")}>
        <HubPurchases refreshKey={refreshKey} />
      </div>
    </div>
  );
}

