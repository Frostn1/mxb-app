import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Loader2, MapPin, RefreshCw, Search, Server, Users } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/Components/ui/button";
import { Input } from "@/Components/ui/input";
import HelpHint from "@/Components/ui/help-hint";
import { cpBrowse, joinServer, type BrowseServer } from "../../api/mods";
import { useGameRunning } from "../../lib/useGameRunning";
import { useT } from "../../i18n/context";

/** Remembers the last address, so rejoining a server that isn't listed is one keystroke. */
const LAST_ADDRESS_KEY = "mxb:lastServerAddress";

/**
 * How often the list re-reads itself while it is on screen.
 *
 * A rider drops out of the count ten minutes after their app last reported, so a slower
 * refresh than this would show a grid that had already emptied. Faster buys nothing: the
 * reports it counts arrive every three minutes.
 */
const REFRESH_MS = 45_000;

/**
 * The server browser.
 *
 * MX Bikes has its own list and the app cannot read it: it comes from PiBoSo's master server
 * over a protocol we don't speak, which is why joining a server was a box wanting an IP
 * address for so long. What the app *can* see is where its own users are — every copy in a
 * session already reports the server it is on, by the folded name FrostMod reads out of the
 * running game, because that is how paint sync and voice know who shares your grid. Counting
 * those reports is a live list of where people actually are, and it costs the people on it
 * nothing.
 *
 * So a row here is one of two things:
 *
 * - **Listed** — somebody registered it. It has an address, so Join launches the game
 *   straight into it, and it is on the list whether or not anyone is riding.
 * - **Live** — nobody registered it; it is here because riders running the app are on it.
 *   It has a name, a track and a head count, and no address, so the game's own browser is
 *   still how you get there. Saying where everyone is, is worth doing on its own.
 *
 * The count is a floor and says so: it counts riders running MXB App with paint sync on, not
 * the grid. A server showing nobody may well be full.
 */
export default function ServerBrowser() {
  const t = useT();
  const [servers, setServers] = useState<BrowseServer[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [query, setQuery] = useState("");
  /** Which row's Join is in flight, so only that button spins. */
  const [joining, setJoining] = useState<string | null>(null);
  const [address, setAddress] = useState(
    () => localStorage.getItem(LAST_ADDRESS_KEY) ?? "",
  );
  const { running: gameRunning, refresh: refreshGame } = useGameRunning();

  const load = useCallback(async () => {
    setRefreshing(true);
    try {
      setServers(await cpBrowse());
      setError(null);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setRefreshing(false);
    }
  }, []);

  // On arrival, and then on its own while the page is open: the whole content of this list
  // is who is riding right now, and nobody is going to press Refresh to find out.
  useEffect(() => {
    void load();
    const id = setInterval(() => void load(), REFRESH_MS);
    return () => clearInterval(id);
  }, [load]);

  const join = async (target: string, id: string) => {
    if (joining || !target.trim()) return;
    setJoining(id);
    try {
      const outcome = await joinServer(target);
      if (outcome === "already_running") {
        toast.info(t("join.alreadyRunning"));
      } else {
        localStorage.setItem(LAST_ADDRESS_KEY, target.trim());
        toast.success(t("join.launching", { address: target.trim() }));
        refreshGame();
      }
    } catch (e) {
      toast.error(t("join.failed"), { description: String(e) });
    }
    setJoining(null);
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void join(address, "address");
  };

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return servers ?? [];
    return (servers ?? []).filter((s) =>
      `${s.name} ${s.track ?? ""} ${s.address ?? ""}`.toLowerCase().includes(needle),
    );
  }, [servers, query]);

  const riding = (servers ?? []).reduce((n, s) => n + s.riders, 0);

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">{t("nav.servers")}</h1>
          <HelpHint title={t("nav.servers")} description={t("browser.help")} />
        </div>
        {riding > 0 && (
          <span className="text-[12px] text-muted-foreground">
            {t("browser.ridingNow", { count: riding })}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              spellCheck={false}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("browser.search")}
              className="h-8 w-[190px] pl-8 text-[12.5px]"
            />
          </div>
          <Button variant="outline" size="sm" onClick={() => void load()} disabled={refreshing}>
            <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />{" "}
            {t("browser.refresh")}
          </Button>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-8">
        {error ? (
          <p className="select-text py-16 text-center text-[13px] text-destructive">{error}</p>
        ) : servers === null ? (
          <div className="space-y-1.5 pt-1">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-[58px] animate-pulse rounded-lg bg-white/[0.04]" />
            ))}
          </div>
        ) : shown.length === 0 ? (
          <div className="py-16 text-center">
            <Server className="mx-auto size-7 text-muted-foreground/60" />
            <p className="mt-3 text-[13.5px] font-medium">
              {servers.length === 0 ? t("browser.empty") : t("browser.noMatch")}
            </p>
            {servers.length === 0 && (
              <p className="mx-auto mt-1.5 max-w-[440px] text-[12.5px] leading-relaxed text-muted-foreground">
                {t("browser.emptyWhy")}
              </p>
            )}
          </div>
        ) : (
          <div className="space-y-1.5 pt-1">
            {shown.map((s) => (
              <Row
                key={s.id}
                server={s}
                joining={joining === s.id}
                disabled={joining !== null || gameRunning}
                gameRunning={gameRunning}
                onJoin={() => s.address && void join(s.address, s.id)}
              />
            ))}
          </div>
        )}

        {/* The escape hatch, and the only way onto a server nobody here has ridden yet. It
            is at the bottom rather than behind a link: an address somebody was given in
            Discord is still how most private races are joined. */}
        <form onSubmit={onSubmit} className="mt-7 rounded-xl border border-white/[0.07] p-4">
          <p className="text-[13px] font-semibold">{t("browser.byAddress")}</p>
          <p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
            {t("browser.byAddressWhy")}
          </p>
          <div className="mt-3 flex items-center gap-2">
            <Input
              value={address}
              spellCheck={false}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="203.0.113.10:54210"
              className="h-9 max-w-[280px]"
            />
            <Button
              type="submit"
              size="sm"
              disabled={joining !== null || gameRunning || !address.trim()}
            >
              {joining === "address" && <Loader2 className="size-3.5 animate-spin" />}
              {joining === "address" ? t("join.joining") : t("join.action")}
            </Button>
          </div>
          {gameRunning && (
            <p className="mt-2.5 text-[11.5px] text-muted-foreground">
              {t("join.alreadyRunning")}
            </p>
          )}
        </form>
      </div>
    </div>
  );
}

/** One server. Joinable rows carry a button; the rest say why they don't. */
function Row({
  server,
  joining,
  disabled,
  gameRunning,
  onJoin,
}: {
  server: BrowseServer;
  joining: boolean;
  disabled: boolean;
  gameRunning: boolean;
  onJoin: () => void;
}) {
  const t = useT();
  const busy = server.riders > 0;
  return (
    <div className="flex items-center gap-3 rounded-lg border border-white/[0.07] px-3.5 py-2.5">
      {/* Lit when somebody is on it. The one thing this page exists to show is which of
          these has anyone on it, so it is the first thing in the row. */}
      <span
        title={busy ? t("browser.riders", { count: server.riders }) : t("browser.nobody")}
        className={cn(
          "size-[7px] flex-none rounded-full",
          busy ? "bg-success" : "bg-muted-foreground/30",
        )}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13.5px] font-medium">{server.name}</span>
          {!server.registered && (
            <span
              title={t("browser.liveWhy")}
              className="flex-none rounded border border-white/[0.09] px-1.5 py-px text-[10px] uppercase tracking-wide text-muted-foreground"
            >
              {t("browser.live")}
            </span>
          )}
        </div>
        <div className="mt-0.5 flex items-center gap-3 text-[11.5px] text-muted-foreground">
          <span className="flex items-center gap-1">
            <Users className="size-3" />
            {busy ? t("browser.riders", { count: server.riders }) : t("browser.nobody")}
          </span>
          {server.track && (
            <span className="flex min-w-0 items-center gap-1">
              <MapPin className="size-3 flex-none" />
              <span className="truncate">{server.track}</span>
            </span>
          )}
          {server.address && <span className="truncate">{server.address}</span>}
        </div>
      </div>
      {server.region && (
        <span className="flex-none text-[11px] uppercase text-muted-foreground">
          {server.region}
        </span>
      )}
      {server.address ? (
        <Button
          size="sm"
          variant="outline"
          disabled={disabled}
          // A greyed-out Join is nearly always the game already being up, and the connect
          // flag is only read at startup — so say so rather than leaving it a mystery.
          title={gameRunning ? t("join.alreadyRunning") : undefined}
          onClick={onJoin}
        >
          {joining && <Loader2 className="size-3.5 animate-spin" />}
          {joining ? t("join.joining") : t("join.action")}
        </Button>
      ) : (
        // No address to launch at: the rider who put this server on the list picked it out
        // of the game's own browser, and the game never told their app where it is.
        <span className="flex-none text-[11.5px] text-muted-foreground">
          {t("browser.pickInGame")}
        </span>
      )}
    </div>
  );
}
