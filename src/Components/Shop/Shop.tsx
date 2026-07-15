import { useCallback, useEffect, useState } from "react";
import { Store, LogOut, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import {
  getInstalledMods,
  normalizeModName,
  onShopAuth,
  shopLogin,
  shopLogout,
  shopMyDownloads,
  shopStatus,
  type ShopItem,
} from "../../api/mods";
import { useInstall } from "../../Context/Install";
import ModCard from "../Browse/ModCard";
import { Button } from "@/Components/ui/button";
import { Skeleton } from "@/Components/ui/skeleton";

interface ShopProps {
  /** Bumped after any install so the "already installed" badges re-scan. */
  refreshKey: number;
}

export default function Shop({ refreshKey }: ShopProps) {
  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  const [items, setItems] = useState<ShopItem[]>([]);
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { startShopInstall } = useInstall();

  const loadDownloads = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await shopMyDownloads();
      setItems(list);
    } catch (e) {
      const message = String(e);
      setError(message);
      // A stale session surfaces as an auth error — drop back to signed-out.
      if (/sign in|session/i.test(message)) setLoggedIn(false);
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial status probe.
  useEffect(() => {
    let cancelled = false;
    shopStatus()
      .then((ok) => {
        if (cancelled) return;
        setLoggedIn(ok);
        if (ok) void loadDownloads();
      })
      .catch(() => !cancelled && setLoggedIn(false));
    return () => {
      cancelled = true;
    };
  }, [loadDownloads]);

  // WebView sign-in completion.
  useEffect(() => {
    const unlisten = onShopAuth((ok) => {
      if (ok) {
        setLoggedIn(true);
        toast.success("Signed in to MX Bikes Shop");
        void loadDownloads();
      } else {
        toast.error("Couldn't capture your MX Bikes Shop session");
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadDownloads]);

  // Keep the "installed" badges in sync with the tracks library.
  useEffect(() => {
    let cancelled = false;
    getInstalledMods("mods/tracks")
      .then((installed) => {
        if (cancelled) return;
        setInstalledNames(new Set(installed.map((m) => normalizeModName(m.name))));
      })
      .catch(() => !cancelled && setInstalledNames(new Set()));
    return () => {
      cancelled = true;
    };
  }, [refreshKey, loggedIn]);

  const install = useCallback(
    (item: ShopItem) => {
      startShopInstall(item);
      toast.success(`Queued “${item.title}”`, {
        description: "Installing to your tracks folder.",
      });
    },
    [startShopInstall],
  );

  const logout = useCallback(async () => {
    await shopLogout();
    setLoggedIn(false);
    setItems([]);
    setError(null);
  }, []);

  // Signed-out gate.
  if (loggedIn === false) {
    return (
      <div className="flex h-full flex-col">
        <Header />
        <div className="flex flex-1 flex-col items-center justify-center gap-4 px-7 text-center">
          <div className="grid size-14 place-items-center rounded-2xl bg-foreground/[0.06] text-foreground/50">
            <Store className="size-7" strokeWidth={1.5} />
          </div>
          <div className="flex max-w-sm flex-col gap-1.5">
            <h2 className="text-[15px] font-semibold">Sign in to MX Bikes Shop</h2>
            <p className="text-[12.5px] text-muted-foreground">
              Log in to mxbikes-shop.com to see and install the tracks
              you&apos;ve purchased. We open the real site — your password never
              touches this app.
            </p>
          </div>
          <Button onClick={() => void shopLogin()}>
            <Store className="size-4" /> Sign in
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <Header
        right={
          loggedIn ? (
            <div className="ml-auto flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void loadDownloads()}
                disabled={loading}
              >
                <RefreshCw className="size-3.5" /> Refresh
              </Button>
              <Button variant="outline" size="sm" onClick={() => void logout()}>
                <LogOut className="size-3.5" /> Log out
              </Button>
            </div>
          ) : undefined
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <div className="flex flex-col items-center gap-3 py-20 text-center">
            <p className="text-[13px] text-destructive">
              Couldn&apos;t load your downloads: {error}
            </p>
            <Button variant="outline" size="sm" onClick={() => void loadDownloads()}>
              Retry
            </Button>
          </div>
        ) : loading || loggedIn === null ? (
          <div className="grid grid-cols-4 gap-3.5">
            {Array.from({ length: 8 }).map((_, i) => (
              <Skeleton key={i} className="aspect-[4/3] rounded-xl" />
            ))}
          </div>
        ) : items.length === 0 ? (
          <p className="py-20 text-center text-[13px] text-muted-foreground">
            No purchased downloads found on your account yet.
          </p>
        ) : (
          <div className="grid grid-cols-4 gap-3.5">
            {items.map((item) => (
              <ModCard
                key={item.id}
                mod={item}
                isBike={false}
                installed={installedNames.has(normalizeModName(item.title))}
                selected={false}
                selectionActive={false}
                onOpen={() => install(item)}
                onToggleSelect={() => {}}
                onQuickInstall={() => install(item)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Header({ right }: { right?: React.ReactNode }) {
  return (
    <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
      <h1 className="text-[21px] font-bold tracking-[-0.2px]">Shop</h1>
      <span className="text-[12.5px] text-muted-foreground">My Downloads</span>
      {right}
    </header>
  );
}
