import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  EyeOff,
  Palette,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { ContextBarRight } from "../Shell/ContextBar";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import {
  listMasterServers,
  joinServer,
  serversWithPaintSync,
  type MasterServer,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import JoinServerDialog from "../Shell/JoinServerDialog";
import ServerDetail from "./ServerDetail";

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
  const [detail, setDetail] = useState<MasterServer | null>(null);
  // Spam and cheat-advertising servers are marked by the backend, not dropped, so this can
  // reveal them. Off by default: the whole point is not to have to read past them.
  const [showHidden, setShowHidden] = useState(false);
  // Riders running paint sync, by address. Which rows are worth joining used to mean opening
  // each one and reading its Riders panel — a request per server to answer a question about
  // the list. This is one request for all of them, and it marks the rows.
  const [paintSync, setPaintSync] = useState<Record<string, number>>({});

  // One fetch at a time. Two overlapping ones each sign in to Steam, and the loser's
  // failure used to replace the winner's list with an error.
  const inFlight = useRef(false);
  // What the tab is showing, for the failure path — `load` holds no state of its own.
  const onScreen = useRef<MasterServer[] | null>(null);
  const load = useCallback(() => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    setError(null);
    listMasterServers()
      .then((list) => {
        // Busiest first — an empty server is the last thing anyone's looking for.
        list.sort((a, b) => b.players - a.players);
        onScreen.current = list;
        setServers(list);
        // After the list, never with it: the browser has to draw whether or not the control
        // plane answers, and badges arriving a moment later is the right trade for that.
        serversWithPaintSync(
          list.map((s) => ({ name: s.name, address: s.address })),
        )
          .then(setPaintSync)
          .catch(() => setPaintSync({}));
      })
      .catch((e: unknown) => {
        const message = typeof e === "string" ? e : String(e);
        setError(message);
        // A failed refresh is not an empty list: keep what's on screen and say so instead.
        if (onScreen.current?.length) toast.error(message);
        else setServers([]);
      })
      .finally(() => {
        inFlight.current = false;
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  /** How many rows the filter caught, whether or not they're being shown. */
  const hiddenCount = useMemo(
    () => (servers ?? []).filter((s) => s.hidden).length,
    [servers],
  );

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const visible = (servers ?? []).filter((s) => showHidden || !s.hidden);
    if (!q) return visible;
    return visible.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.track.toLowerCase().includes(q) ||
        s.location.toLowerCase().includes(q) ||
        s.address.toLowerCase().includes(q),
    );
  }, [servers, query, showHidden]);

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
      <ContextBarRight>
        {servers && servers.length > 0 && (
          <span className="tabular-figures text-[12.5px] text-faint">
            {t("serverBrowser.count", { count: servers.length - (showHidden ? 0 : hiddenCount) })}
          </span>
        )}
        {hiddenCount > 0 && (
          <button
            type="button"
            onClick={() => setShowHidden((v) => !v)}
            title={t("serverBrowser.hiddenHelp")}
            className={cn(
              "flex h-7 items-center gap-1.5 border border-input px-2.5 text-[12px]",
              showHidden ? "bg-card text-muted-foreground" : "text-faint hover:text-muted-foreground",
            )}
          >
            <EyeOff className="size-3.5" />
            {showHidden
              ? t("serverBrowser.hideFiltered")
              : t("serverBrowser.hiddenCount", { count: hiddenCount })}
          </button>
        )}
        <div className="flex h-7 w-[220px] items-center gap-2 border border-input bg-card px-2.5">
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
        <HelpHint title={t("servers.title")} description={t("serverBrowser.help")} />
      </ContextBarRight>

      <JoinServerDialog open={joinOpen} onOpenChange={setJoinOpen} onJoined={load} />
      <ServerDetail
        server={detail}
        onOpenChange={(open) => !open && setDetail(null)}
        onJoin={join}
        joining={joining}
      />

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {servers === null ? (
          <Centered>
            <Loader2 className="size-5 animate-spin text-faint" />
            <p className="text-[13px] text-faint">{t("serverBrowser.loading")}</p>
          </Centered>
        ) : error && servers.length === 0 ? (
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
                  <th className="px-2 py-2.5 font-semibold">{t("serverBrowser.location")}</th>
                  <th className="w-[72px] px-2 py-2.5 font-semibold">{t("serverBrowser.ping")}</th>
                  <th className="px-2 py-2.5 font-semibold">{t("serverBrowser.address")}</th>
                  <th className="w-[110px] px-3.5 py-2.5" />
                </tr>
              </thead>
              <tbody>
                {shown.map((s, i) => (
                  <tr
                    key={`${s.address}-${i}`}
                    onClick={() => setDetail(s)}
                    className={cn(
                      "cursor-pointer border-b border-input/60 last:border-0 hover:bg-foreground/[0.03]",
                      // Revealed rows stay legible but visibly demoted, so nobody mistakes one
                      // for an ordinary result they just hadn't scrolled to.
                      s.hidden && "opacity-55",
                    )}
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
                        {s.hidden && (
                          <span
                            className="shrink-0 border border-input px-1.5 py-px text-[10.5px] uppercase tracking-wide text-faint"
                            title={t("serverBrowser.hiddenBecause", { reason: s.hidden })}
                          >
                            {t("serverBrowser.filtered")}
                          </span>
                        )}
                        {(paintSync[s.address] ?? 0) > 0 && (
                          <span
                            className="inline-flex shrink-0 items-center gap-1 border border-success/40 bg-success/10 px-1.5 py-px text-[10.5px] tabular-nums text-success"
                            title={t("serverBrowser.paintSyncHere", {
                              count: paintSync[s.address],
                            })}
                          >
                            <Palette className="size-3" />
                            {paintSync[s.address]}
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-2 py-2.5 tabular-nums text-muted-foreground">
                      <span className="inline-flex items-center gap-1.5">
                        <Users className="size-3.5 text-faint" />
                        {s.players}/{s.maxPlayers}
                      </span>
                    </td>
                    <td className="px-2 py-2.5 text-muted-foreground">
                      <span
                        className="block max-w-[220px] truncate"
                        title={[s.track, s.trackLayout].filter(Boolean).join(" — ")}
                      >
                        {s.track || "—"}
                        {s.trackLayout && (
                          <span className="text-faint"> · {s.trackLayout}</span>
                        )}
                      </span>
                    </td>
                    <td className="px-2 py-2.5 text-muted-foreground">
                      <span className="block max-w-[140px] truncate" title={s.location}>
                        {s.location || "—"}
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
                        onClick={(e) => {
                          e.stopPropagation();
                          copy(s.address);
                        }}
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
                        onClick={(e) => {
                          e.stopPropagation();
                          join(s.address);
                        }}
                        disabled={joining !== null || !s.joinable}
                        title={s.joinable ? undefined : t("serverBrowser.notJoinable")}
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
