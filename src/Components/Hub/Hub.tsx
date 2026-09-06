import { useState } from "react";
import HubCatalog from "./HubCatalog";
import HubPurchases from "./HubPurchases";
import HelpHint from "@/Components/ui/help-hint";
import { cn } from "@/lib/utils";
import { useT } from "../../i18n/context";

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
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">{t("nav.hub")}</h1>
        <HelpHint title={t("nav.hub")} description={t("hub.help")} />

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
      </header>

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
