/**
 * Every sign-in the app holds, in one place.
 *
 * They used to be scattered: Steam sat in Secure content, and the two store sign-ins were
 * buried inside each store's own Purchases tab, which meant the only way to find out whether
 * you were signed in to MXB Hub was to go and look at MXB Hub. Four accounts, one of which is
 * deliberately no account at all, is a list — so it reads as one.
 *
 * The rows are honest about what is behind them. mxbikes-shop.com is read through a hidden
 * browser window because its Cloudflare challenge refuses an HTTP client, and that is slow;
 * MXB Hub is plain HTML over the normal client and is not. Neither store announces a purchase,
 * which is why the switch at the bottom exists and why it says "watches" rather than promising
 * anything instant.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Globe,
  Info,
  Loader2,
  LogIn,
  LogOut,
  RefreshCw,
  ShieldCheck,
  Store,
} from "lucide-react";
import { toast } from "sonner";
import {
  onShopAuth,
  shopLogin,
  shopLogout,
  shopMyDownloads,
  shopStatus,
  steamLinkStatus,
} from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { Switch } from "@frost/shared/Components/ui/switch";
import { cn } from "@frost/shared/lib/utils";
import {
  hubLogin,
  hubLogout,
  hubMyDownloads,
  hubStatus,
  onHubAuth,
} from "@/api/hub";
import type { StoreId } from "@/api/shop";
import {
  forgetStoreCount,
  readAutoInstall,
  readStoreCounts,
  writeAutoInstall,
  writeStoreCount,
} from "@/lib/autoInstall";
import { useSteamLink } from "@/lib/useSteamLink";
import { useT } from "@/i18n";

/** Where a row's status dot sits on the three states any account can be in. */
type Tone = "on" | "off" | "none";

const DOT: Record<Tone, string> = {
  on: "bg-success",
  off: "bg-warning",
  // Nothing to sign into, so nothing to warn about — mxb-mods needs no account.
  none: "bg-foreground/25",
};

export default function Accounts() {
  const t = useT();
  const tRef = useRef(t);
  tRef.current = t;

  const [steamId, setSteamId] = useState<string | null>(null);
  const { linking, linkSteam } = useSteamLink({ onLinked: setSteamId });

  const [shopIn, setShopIn] = useState<boolean | null>(null);
  const [hubIn, setHubIn] = useState<boolean | null>(null);
  /** What each store held the last time anything managed to read it. */
  const [counts, setCounts] = useState(readStoreCounts);
  /** The store whose Refresh is in flight, if any. */
  const [refreshing, setRefreshing] = useState<StoreId | null>(null);
  const [autoInstall, setAutoInstall] = useState(readAutoInstall);

  useEffect(() => {
    let cancelled = false;
    steamLinkStatus()
      .then((id) => !cancelled && setSteamId(id))
      .catch(() => {});
    shopStatus()
      .then((ok) => !cancelled && setShopIn(ok))
      .catch(() => !cancelled && setShopIn(false));
    hubStatus()
      .then((ok) => !cancelled && setHubIn(ok))
      .catch(() => !cancelled && setHubIn(false));
    return () => {
      cancelled = true;
    };
  }, []);

  // Both stores sign in through a window of their own, so the answer arrives as an event
  // rather than from the call that opened it.
  useEffect(() => {
    const shop = onShopAuth((ok) => {
      setShopIn(ok);
      if (ok) toast.success(tRef.current("shop.signedIn"));
      else toast.error(tRef.current("shop.sessionFailed"));
    });
    const hub = onHubAuth((ok) => {
      setHubIn(ok);
      if (ok) toast.success(tRef.current("hub.signedIn"));
      else toast.error(tRef.current("hub.sessionFailed"));
    });
    return () => {
      void shop.then((fn) => fn());
      void hub.then((fn) => fn());
    };
  }, []);

  const toggleAuto = useCallback((on: boolean) => {
    writeAutoInstall(on);
    setAutoInstall(on);
  }, []);

  /** Re-read a store's purchases, purely to put a number on the row. The shop's read drives
   *  the parked WebView through Cloudflare again, which is why the button spins. */
  const refresh = useCallback(async (store: StoreId) => {
    setRefreshing(store);
    try {
      const products =
        store === "shop"
          ? new Set((await shopMyDownloads(true)).map((i) => i.product))
          : new Set((await hubMyDownloads()).items.map((i) => i.product));
      writeStoreCount(store, products.size);
      setCounts(readStoreCounts());
    } catch (e) {
      toast.error(tRef.current("accounts.refreshFailed"), { description: String(e) });
    } finally {
      setRefreshing(null);
    }
  }, []);

  const signOut = useCallback(async (store: StoreId) => {
    try {
      if (store === "shop") {
        await shopLogout();
        setShopIn(false);
      } else {
        await hubLogout();
        setHubIn(false);
      }
      forgetStoreCount(store);
      setCounts(readStoreCounts());
    } catch (e) {
      toast.error(tRef.current("accounts.signOutFailed"), { description: String(e) });
    }
  }, []);

  /** "Signed in · 12 purchases", with the count left off until something has read one. */
  const storeStatus = useCallback(
    (store: StoreId, signedIn: boolean | null) => {
      if (signedIn === null) return t("accounts.checking");
      if (!signedIn) return t("accounts.signedOut");
      const count = counts[store];
      return count === undefined
        ? t("accounts.countUnknown")
        : `${t("accounts.signedIn")} · ${t("accounts.purchases", { count })}`;
    },
    [counts, t],
  );

  const storeRow = (store: StoreId, name: string, signedIn: boolean | null) => (
    <Row
      icon={<Store className="size-4" />}
      tone={signedIn ? "on" : "off"}
      name={name}
      status={storeStatus(store, signedIn)}
      note={store === "shop" ? t("accounts.shopNote") : t("accounts.hubNote")}
    >
      {signedIn && (
        <Button
          variant="outline"
          size="sm"
          disabled={refreshing !== null}
          onClick={() => void refresh(store)}
        >
          {refreshing === store ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <RefreshCw className="size-3.5" />
          )}
          {t("common.refresh")}
        </Button>
      )}
      {signedIn ? (
        <Button variant="ghost" size="sm" onClick={() => void signOut(store)}>
          <LogOut className="size-3.5" /> {t("accounts.signOut")}
        </Button>
      ) : (
        <Button
          size="sm"
          onClick={() => void (store === "shop" ? shopLogin() : hubLogin())}
        >
          <LogIn className="size-3.5" /> {t("accounts.signIn")}
        </Button>
      )}
    </Row>
  );

  return (
    <div className="flex flex-col gap-1">
      <Row
        icon={<ShieldCheck className="size-4" />}
        tone={steamId ? "on" : "off"}
        name={t("accounts.steam")}
        status={
          steamId ? t("settings.steamLinkedAs", { id: steamId }) : t("accounts.steamNot")
        }
        note={t("accounts.steamWhat")}
      >
        <Button
          size="sm"
          variant={steamId ? "outline" : "default"}
          disabled={linking}
          onClick={() => void linkSteam()}
        >
          {linking
            ? t("settings.steamLinking")
            : steamId
              ? t("settings.steamRelink")
              : t("settings.steamLinkBtn")}
        </Button>
      </Row>

      {storeRow("shop", t("accounts.shop"), shopIn)}
      {storeRow("hub", t("accounts.hub"), hubIn)}

      <Row
        icon={<Globe className="size-4" />}
        tone="none"
        name={t("accounts.mods")}
        status={t("accounts.modsNone")}
        note={t("accounts.modsNote")}
      />

      <div className="mt-3 flex items-start justify-between gap-4 border-t border-border/60 pt-4">
        <div className="flex flex-col gap-0.5">
          <span className="text-[12.5px] text-foreground/85">
            {t("accounts.autoInstall")}
          </span>
          <span className="text-[11.5px] leading-relaxed text-muted-foreground">
            {t("accounts.autoInstallDesc")}
          </span>
        </div>
        <div className="pt-0.5">
          <Switch checked={autoInstall} onCheckedChange={toggleAuto} />
        </div>
      </div>

      {/* Under the switch, because it is what the switch is actually doing. Neither store can
          be asked "what's new?", so the honest word for this is watching. */}
      <div className="mt-2 flex items-start gap-2.5 rounded-lg border border-input bg-foreground/[0.03] p-3">
        <Info className="mt-[1px] size-4 flex-none text-muted-foreground" />
        <div className="flex flex-col gap-0.5">
          <span className="font-cond text-[12px] font-semibold tracking-[-0.01em] text-foreground/85">
            {t("accounts.readTitle")}
          </span>
          <span className="text-[11.5px] leading-relaxed text-muted-foreground">
            {t("accounts.readBody")}
          </span>
        </div>
      </div>
    </div>
  );
}

/** One account: a dot that says whether it is live, what it is, where it stands, and its own
 *  buttons. The buttons are grouped on the right of the row rather than under it, so four
 *  accounts read as four lines and not as four cards. */
function Row({
  icon,
  tone,
  name,
  status,
  note,
  children,
}: {
  icon: React.ReactNode;
  tone: Tone;
  name: string;
  status: string;
  /** The line under the status: what this account is actually for, or how it is read. */
  note?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4 rounded-lg px-1 py-2.5">
      <div className="flex min-w-0 gap-3">
        <span className="mt-0.5 grid size-8 flex-none place-items-center rounded-lg bg-foreground/[0.06] text-foreground/60">
          {icon}
        </span>
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="flex items-center gap-2 font-cond text-[12.5px] font-semibold tracking-[-0.01em]">
            <span className={cn("inline-block size-2 flex-none rounded-full", DOT[tone])} />
            {name}
          </span>
          <span className="text-[12px] text-muted-foreground">{status}</span>
          {note && (
            <span className="text-[11.5px] leading-relaxed text-faint">{note}</span>
          )}
        </div>
      </div>
      {children && <div className="flex flex-none items-center gap-1.5">{children}</div>}
    </div>
  );
}
