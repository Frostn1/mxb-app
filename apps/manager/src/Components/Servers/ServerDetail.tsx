import { useEffect, useState } from "react";
import { Lock, Plug, Loader2, Signal, Users, Download, MapPin, CheckCircle2 } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import { useT } from "@/i18n";
import {
  probeServer,
  serverRiders,
  guessServerTrack,
  type MasterServer,
  type ServerRiders,
  type TrackGuess,
} from "@frost/shared/api/mods";

/**
 * Everything one server publishes about itself.
 *
 * The list row stays to what a player scans for — who's on, how far away, which track. The
 * rest of it is real data the master has always carried and we used to throw away: the
 * event blob holds the track, the session and the rules, and `location` and the licence
 * class sit beside it. It lands here rather than in the row because it is what you read
 * once, before deciding to join, not what you compare fifty rows on.
 *
 * Three things arrive after opening, because none of them is worth fetching for every row in
 * a list: the server's own live answer, who the app can name on it, and which track the
 * internal id it publishes actually refers to.
 */

/** One label/value line. Values that came back empty are dropped by {@link Facts}. */
type Fact = { label: string; value: string };

const Facts = ({ title, facts }: { title: string; facts: Fact[] }) => {
  const shown = facts.filter((f) => f.value);
  if (shown.length === 0) return null;
  return (
    <section className="space-y-2">
      <h3 className="text-[11.5px] font-semibold uppercase tracking-wide text-faint">
        {title}
      </h3>
      <dl className="grid grid-cols-[minmax(0,7rem)_1fr] gap-x-4 gap-y-1.5 text-[13px]">
        {shown.map((f) => (
          <div key={f.label} className="contents">
            <dt className="truncate text-muted-foreground">{f.label}</dt>
            <dd className="min-w-0 break-words">{f.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
};

/**
 * Who is on the server.
 *
 * The count is the server's own and is always right. The names are not the same thing and
 * must not look like they are: MX Bikes tells a stranger how many riders are on and nothing
 * else, so unless this is the server under you, the names are the riders whose own copy of
 * Frost's Mod Manager said they were here. That is a subset, and the label says so.
 */
const Riders = ({
  players,
  maxPlayers,
  riders,
  loading,
}: {
  players: number;
  maxPlayers: number;
  riders: ServerRiders | null;
  loading: boolean;
}) => {
  const t = useT();
  const names = riders?.riders ?? [];
  return (
    <section className="space-y-2">
      <h3 className="flex items-center gap-2 text-[11.5px] font-semibold uppercase tracking-wide text-faint">
        {t("serverBrowser.ridersTitle")}
        {loading && <Loader2 className="size-3 animate-spin" />}
      </h3>
      <p className="text-[13px]">
        {t("serverBrowser.ridersCount", { players, maxPlayers })}
        {names.length > 0 && (
          <span className="text-muted-foreground">
            {" · "}
            {riders?.source === "session"
              ? t("serverBrowser.ridersFromSession")
              : t("serverBrowser.ridersFromApp", { count: names.length })}
          </span>
        )}
      </p>
      {names.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {names.map((n) => (
            <span
              key={n}
              className="border border-input bg-card px-2 py-0.5 text-[12px] text-muted-foreground"
            >
              {n}
            </span>
          ))}
        </div>
      ) : (
        !loading &&
        players > 0 && (
          <p className="text-[12px] text-faint">{t("serverBrowser.ridersUnknown")}</p>
        )
      )}
    </section>
  );
};

/**
 * Which track this actually is.
 *
 * A server publishes an internal id — `mmx_supercross` — which is not a title, not a folder
 * name and not something anyone can search for. Having it is the best answer and shows the
 * track's own artwork, whether it is installed or came with the game; otherwise this offers
 * where to get it, and says plainly when the name only resembles a product rather than
 * matching it.
 */
const Track = ({ guess, loading }: { guess: TrackGuess | null; loading: boolean }) => {
  const t = useT();
  if (loading) {
    return (
      <p className="flex items-center gap-2 text-[12.5px] text-faint">
        <Loader2 className="size-3.5 animate-spin" />
        {t("serverBrowser.trackChecking")}
      </p>
    );
  }
  if (!guess || (!guess.installed && !guess.source)) return null;

  const art = guess.preview || guess.productImage;
  return (
    <section className="space-y-2">
      <h3 className="text-[11.5px] font-semibold uppercase tracking-wide text-faint">
        {t("serverBrowser.trackTitle")}
      </h3>
      <div className="flex items-start gap-3">
        {art && (
          <img
            src={art}
            alt=""
            className="h-[72px] w-[128px] shrink-0 border border-input object-cover"
          />
        )}
        <div className="min-w-0 flex-1 space-y-1.5">
          {guess.installed ? (
            <p className="flex items-center gap-1.5 text-[13px]">
              <CheckCircle2 className="size-3.5 shrink-0 text-faint" />
              <span className="truncate">
                {guess.stock
                  ? t("serverBrowser.trackStock", { name: guess.installed })
                  : t("serverBrowser.trackInstalled", { name: guess.installed })}
              </span>
            </p>
          ) : (
            <>
              <p className="text-[13px]">
                <MapPin className="mr-1.5 inline size-3.5 text-faint" />
                {guess.exact
                  ? guess.productName
                  : t("serverBrowser.trackMaybe", { name: guess.productName })}
              </p>
              {guess.productUrl && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void openUrl(guess.productUrl)}
                >
                  <Download className="size-3.5" />
                  {guess.source === "mods"
                    ? t("serverBrowser.trackGetMods")
                    : guess.source === "hub"
                      ? t("serverBrowser.trackGetHub")
                      : t("serverBrowser.trackGetShop")}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
    </section>
  );
};

const ServerDetail = ({
  server,
  onOpenChange,
  onJoin,
  joining,
}: {
  server: MasterServer | null;
  onOpenChange: (open: boolean) => void;
  onJoin: (address: string) => void;
  joining: string | null;
}) => {
  const t = useT();
  // What the row carried, replaced by the server's own answer once it arrives. Held here
  // rather than pushed back into the list: the list refreshes on its own schedule, and one
  // row updating under a player's cursor while they read it would be worse than stale.
  const [live, setLive] = useState<MasterServer | null>(null);
  const [riders, setRiders] = useState<ServerRiders | null>(null);
  const [ridersLoading, setRidersLoading] = useState(false);
  const [guess, setGuess] = useState<TrackGuess | null>(null);
  const [guessing, setGuessing] = useState(false);

  const address = server?.address ?? "";
  const name = server?.name ?? "";
  const track = live?.track || server?.track || "";

  // Ask the server about itself, and ask who is on it. `cancelled` is what keeps a slow
  // answer for the last server out of the panel for the next one.
  useEffect(() => {
    if (!address) return;
    let cancelled = false;
    setLive(null);
    setRiders(null);
    setRidersLoading(true);
    probeServer(address)
      .then((s) => !cancelled && setLive(s))
      .catch(() => {});
    serverRiders(address, name)
      .then((r) => !cancelled && setRiders(r))
      .catch(() => {})
      .finally(() => !cancelled && setRidersLoading(false));
    return () => {
      cancelled = true;
    };
  }, [address, name]);

  // Separate from the probe because it keys on the track, which the probe can change: a
  // server that rolled over to the next track while the panel was open re-identifies it.
  useEffect(() => {
    if (!track) {
      setGuess(null);
      return;
    }
    let cancelled = false;
    setGuessing(true);
    guessServerTrack(track)
      .then((g) => !cancelled && setGuess(g))
      .catch(() => {})
      .finally(() => !cancelled && setGuessing(false));
    return () => {
      cancelled = true;
    };
  }, [track]);

  if (!server) return null;
  const s = live ?? server;

  const yes = t("serverBrowser.yes");
  const flag = (on: boolean) => (on ? yes : "");
  // An empty allow-list is the server saying it doesn't mind, which is worth stating.
  const list = (xs: string[]) => (xs.length ? xs.join(", ") : t("serverBrowser.any"));

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[560px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 pr-6">
            {s.passworded && <Lock className="size-4 shrink-0 text-faint" />}
            <span className="truncate">{s.name}</span>
          </DialogTitle>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-muted-foreground">
            <span className="inline-flex items-center gap-1.5">
              <Users className="size-3.5 text-faint" />
              {s.players}/{s.maxPlayers}
            </span>
            {s.pingMs !== null && (
              <span className="inline-flex items-center gap-1.5 tabular-nums">
                <Signal className="size-3.5 text-faint" />
                {s.pingMs} ms
              </span>
            )}
            {s.location && <span>{s.location}</span>}
          </div>
        </DialogHeader>

        <div className="max-h-[60vh] space-y-5 overflow-y-auto pr-1">
          <Riders
            players={s.players}
            maxPlayers={s.maxPlayers}
            riders={riders}
            loading={ridersLoading}
          />

          <Track guess={guess} loading={guessing} />

          <Facts
            title={t("serverBrowser.running")}
            facts={[
              { label: t("servers.track"), value: s.track },
              { label: t("serverBrowser.layout"), value: s.trackLayout },
              { label: t("serverBrowser.session"), value: s.session },
              { label: t("serverBrowser.raceLength"), value: s.raceLength },
              { label: t("serverBrowser.conditions"), value: s.conditions },
              {
                label: t("serverBrowser.changingWeather"),
                value: flag(s.realisticWeather),
              },
            ]}
          />

          <Facts
            title={t("serverBrowser.rules")}
            facts={[
              { label: t("serverBrowser.categories"), value: list(s.categories) },
              { label: t("serverBrowser.bikes"), value: list(s.bikes) },
              { label: t("serverBrowser.rating"), value: s.rating },
              { label: t("serverBrowser.forceCockpit"), value: flag(s.forceCockpit) },
              { label: t("serverBrowser.noAids"), value: flag(s.noAids) },
              {
                label: t("serverBrowser.limitedTyres"),
                value: flag(s.limitedTyreSets),
              },
              {
                label: t("serverBrowser.passworded"),
                value: flag(s.passworded),
              },
            ]}
          />

          <Facts
            title={t("serverBrowser.connection")}
            facts={[
              { label: t("serverBrowser.address"), value: s.address },
              { label: t("serverBrowser.lanAddress"), value: s.lanAddress },
            ]}
          />
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-input pt-3">
          {/* A server the game can't be pointed at says so, rather than offering a button
              that fails every time. */}
          <p className="text-[12px] text-faint">
            {s.joinable ? "" : t("serverBrowser.notJoinable")}
          </p>
          <Button
            onClick={() => onJoin(s.address)}
            disabled={!s.joinable || joining !== null}
          >
            {joining === s.address ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Plug className="size-3.5" />
            )}
            {t("serverBrowser.join")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default ServerDetail;
