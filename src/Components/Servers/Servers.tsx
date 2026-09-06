import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Search,
  RefreshCw,
  Loader2,
  Lock,
  Users,
  Signal,
  Copy,
  Plug,
  ServerOff,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { Button } from "@/Components/ui/button";
import HelpHint from "@/Components/ui/help-hint";
import {
  listMasterServers,
  joinServer,
  type MasterServer,
} from "../../api/mods";
import { useT } from "../../i18n/context";
import JoinServerDialog from "../Shell/JoinServerDialog";

/**
 * The live MX Bikes server list, read straight from PiBoSo's master server — the same
 * population the in-game WORLD browser shows, with the IP the game never surfaces. Joining a
 * row reuses the existing `-directconnect` launch, so this is the one-click path the address
 * dialog only hinted at.
 *
 * The fetch is one Rust command; everything the master needs (auth, the protocol) lives
 * behind it. A build without the browser, or a master that won't answer, comes back as a
 * plain error string this renders rather than a blank tab.
 */
const Servers = () => {
  const t = useT();
  const [servers, setServers] = useState<MasterServer[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState("");
  const [joining, setJoining] = useState<string | null>(null);
  const [joinOpen, setJoinOpen] = useState(false);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    listMasterServers()
      .then((list) => {
        // Busiest first — an empty server is the last thing anyone's looking for.
        list.sort((a, b) => b.players - a.players);
        setServers(list);
      })
      .catch((e: unknown) => {
        setServers([]);
        setError(typeof e === "string" ? e : String(e));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q || !servers) return servers ?? [];
    return servers.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.track.toLowerCase().includes(q) ||
        s.address.toLowerCase().includes(q),
    );
  }, [servers, query]);

  const join = useCallback(
    async (address: string) => {
      if (joining) return;
      setJoining(address);
      try {
        const outcome = await joinServer(address);
        if (outcome === "already_running") {
          toast.info(t("join.alreadyRunning"));
        } else {
          toast.success(t("join.launching", { address }));
        }
      } catch (e) {
        toast.error(typeof e === "string" ? e : t("serverBrowser.joinFailed"));
      } finally {
        setJoining(null);
      }
    },
    [joining, t],
  );

  const copy = useCallback(
    (address: string) => {
      navigator.clipboard
        .writeText(address)
        .then(() => toast.success(t("serverBrowser.copied")))
        .catch(() => {});
    },
    [t],
  );

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none flex-col gap-3 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-3.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">
            {t("servers.title")}
          </h1>
          <HelpHint
            title={t("servers.title")}
            description={t("serverBrowser.help")}
          />
          {servers && servers.length > 0 && (
            <span className="text-[12.5px] text-faint">
              {t("serverBrowser.count", { count: servers.length })}
            </span>
          )}
          <div className="ml-auto flex items-center gap-2">
            <div className="flex w-[240px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
              <Search className="size-3.5 text-faint" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("serverBrowser.searchPlaceholder")}
                className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
              />
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={load}
              disabled={loading}
              title={t("serverBrowser.refresh")}
            >
              <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
              {t("serverBrowser.refresh")}
            </Button>
            {/* Join by address, for a server the master list doesn't carry. It lived in the
                sidebar next to Play; with the sidebar gone this is where someone looks for
                it — the page that is already about joining servers. */}
            <Button variant="outline" size="sm" onClick={() => setJoinOpen(true)}>
              <Plug className="size-3.5" />
              {t("join.title")}
            </Button>
          </div>
        </div>
      </header>

      <JoinServerDialog open={joinOpen} onOpenChange={setJoinOpen} onJoined={load} />

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {servers === null ? (
          <Centered>
            <Loader2 className="size-5 animate-spin text-faint" />
            <p className="text-[13px] text-faint">{t("serverBrowser.loading")}</p>
          </Centered>
        ) : error ? (
          <Centered>
            <ServerOff className="size-6 text-faint" />
            <p className="max-w-[420px] text-center text-[13px] text-muted-foreground">
              {error}
            </p>
            <Button variant="outline" size="sm" onClick={load}>
              <RefreshCw className="size-3.5" />
              {t("serverBrowser.retry")}
            </Button>
          </Centered>
        ) : shown.length === 0 ? (
          <Centered>
            <ServerOff className="size-6 text-faint" />
            <p className="text-[13px] text-faint">{t("serverBrowser.empty")}</p>
          </Centered>
        ) : (
          <div className="overflow-hidden rounded-xl border border-input">
            <table className="w-full border-collapse text-[13px]">
              <thead>
                <tr className="border-b border-input bg-card text-left text-[11.5px] uppercase tracking-wide text-faint">
                  <th className="px-3.5 py-2.5 font-semibold">{t("serverBrowser.name")}</th>
                  <th className="w-[92px] px-2 py-2.5 font-semibold">{t("serverBrowser.players")}</th>
                  <th className="px-2 py-2.5 font-semibold">{t("servers.track")}</th>
                  <th className="w-[72px] px-2 py-2.5 font-semibold">{t("serverBrowser.ping")}</th>
                  <th className="px-2 py-2.5 font-semibold">{t("serverBrowser.address")}</th>
                  <th className="w-[110px] px-3.5 py-2.5" />
                </tr>
              </thead>
              <tbody>
                {shown.map((s, i) => (
                  <tr
                    key={`${s.address}-${i}`}
                    className="border-b border-input/60 last:border-0 hover:bg-foreground/[0.03]"
                  >
                    <td className="px-3.5 py-2.5">
                      <div className="flex items-center gap-2">
                        {s.passworded && (
                          <Lock
                            className="size-3.5 shrink-0 text-faint"
                            aria-label={t("serverBrowser.passworded")}
                          />
                        )}
                        <span className="truncate font-medium" title={s.name}>
                          {s.name}
                        </span>
                      </div>
                    </td>
                    <td className="px-2 py-2.5 tabular-nums text-muted-foreground">
                      <span className="inline-flex items-center gap-1.5">
                        <Users className="size-3.5 text-faint" />
                        {s.players}/{s.maxPlayers}
                      </span>
                    </td>
                    <td className="px-2 py-2.5 text-muted-foreground">
                      <span className="block max-w-[220px] truncate" title={s.track}>
                        {s.track || "—"}
                      </span>
                    </td>
                    <td className="px-2 py-2.5 tabular-nums text-muted-foreground">
                      {s.pingMs === null ? (
                        "—"
                      ) : (
                        <span className="inline-flex items-center gap-1.5">
                          <Signal className="size-3.5 text-faint" />
                          {s.pingMs}
                        </span>
                      )}
                    </td>
                    <td className="px-2 py-2.5">
                      <button
                        onClick={() => copy(s.address)}
                        title={t("serverBrowser.copyAddress")}
                        className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 font-mono text-[12px] text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground"
                      >
                        {s.address}
                        <Copy className="size-3 text-faint" />
                      </button>
                    </td>
                    <td className="px-3.5 py-2.5 text-right">
                      <Button
                        size="sm"
                        onClick={() => join(s.address)}
                        disabled={joining !== null}
                      >
                        {joining === s.address ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Plug className="size-3.5" />
                        )}
                        {t("serverBrowser.join")}
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};

/** The three non-list states (loading, error, empty) share this centered column. */
const Centered = ({ children }: { children: React.ReactNode }) => (
  <div className="flex h-full flex-col items-center justify-center gap-3 py-16">
    {children}
  </div>
);

export default Servers;
