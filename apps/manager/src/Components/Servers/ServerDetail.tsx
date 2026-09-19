import { useEffect, useState, type ReactNode } from "react";
import {
  ExternalLink,
  Lock,
  Plug,
  Loader2,
  Signal,
  Users,
  Download,
  MapPin,
  CheckCircle2,
  Hourglass,
  Mountain,
  Copy,
  Star,
  ShoppingCart,
  ServerOff,
} from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "@frost/shared/Components/ui/button";
import { Badge } from "@frost/shared/Components/ui/badge";
import { Dialog, DialogContent, DialogTitle } from "@frost/shared/Components/ui/dialog";
import { cn } from "@frost/shared/lib/utils";
import { useI18n, useT } from "@/i18n";
import {
  probeServer,
  queueCounts,
  serverRiders,
  guessServerTrack,
  type CatalogTrack,
  type MasterServer,
  type QueueState,
  type ServerRiders,
  type TrackGuess,
} from "@frost/shared/api/mods";
import { formatPrice, openShopUrl } from "../../api/shop";
import { isFull } from "@/lib/useServerQueue";

/**
 * Everything one server publishes about itself.
 *
 * The list row stays to what a player scans for — who's on, how far away, which track. The
 * rest of it is real data the master has always carried and we used to throw away: the
 * event blob holds the track, the session and the rules, and `location` and the licence
 * class sit beside it. It lands here rather than in the row because it is what you read
 * once, before deciding to join, not what you compare fifty rows on.
 *
 * This is a pane, not a dialog. It used to open over the grid, so comparing two servers
 * meant closing one to find the other — the one choice being made was hidden behind the
 * thing it was being made from. In list view it now sits beside the list and fills as rows
 * are picked; the tiles open the same pane over the grid, through {@link ServerDetailDialog}.
 *
 * Three things arrive after opening, because none of them is worth fetching for every row in
 * a list: the server's own live answer, who the app can name on it, and which track the
 * internal id it publishes actually refers to.
 */

/** One label/value line. Values that came back empty are dropped by {@link Facts}. */
type Fact = { label: string; value: string };

/** What each track id turned out to be, for the life of the app. Shared by every row: the
 *  same track is on a dozen servers and comes round again on every rotation. */
const GUESSES = new Map<string, TrackGuess>();

const Facts = ({
  title,
  facts,
  columns = 1,
}: {
  title: string;
  facts: Fact[];
  /** Two pairs across. Halves the height of a list nobody reads top to bottom — the rules
   *  are scanned for the one line that would stop you joining, not read in order. */
  columns?: 1 | 2;
}) => {
  const shown = facts.filter((f) => f.value);
  if (shown.length === 0) return null;
  return (
    <section className="space-y-2">
      <h3 className="font-cond text-[11px] font-bold uppercase tracking-[0.14em] text-faint">
        {title}
      </h3>
      <dl
        className={cn(
          "grid gap-x-4 gap-y-1.5 text-[13px]",
          columns === 2
            ? "grid-cols-[minmax(0,7rem)_1fr] xl:grid-cols-[minmax(0,7rem)_1fr_minmax(0,7rem)_1fr]"
            : "grid-cols-[minmax(0,8rem)_1fr]",
        )}
      >
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
 * MXB App said they were here. That is a subset, and the label says so.
 */
const Riders = ({
  riders,
  loading,
  className,
}: {
  riders: ServerRiders | null;
  loading: boolean;
  className?: string;
}) => {
  const t = useT();
  const names = riders?.riders ?? [];
  // The count is already the first figure under the hero. Saying "12 of 30 riders" again
  // here made the section a second copy of it; the names are the only thing this block
  // knows that the band does not, so with no names there is nothing to show.
  if (names.length === 0 && !loading) return null;
  return (
    <section className={cn("space-y-2", className)}>
      <h3 className="flex items-center justify-end gap-2 font-cond text-[11px] font-bold uppercase tracking-[0.14em] text-faint">
        {t("serverBrowser.ridersTitle")}
        {loading && <Loader2 className="size-3 animate-spin" />}
      </h3>
      {names.length > 0 ? (
        <div className="flex flex-wrap justify-end gap-1.5">
          {names.map((n) => (
            <span
              key={n}
              className="rounded-md border border-input bg-card px-2 py-0.5 text-[12px] text-muted-foreground"
            >
              {n}
            </span>
          ))}
        </div>
      ) : null}
    </section>
  );
};

/**
 * Which track this actually is.
 *
 * A server publishes an internal id — `mmx_supercross` — which is not a title, not a folder
 * name and not something anyone can search for. Having it is the best answer; otherwise this
 * offers where to get it, and says plainly when the name only resembles a product rather
 * than matching it. The artwork it used to carry as a thumbnail is the hero above instead.
 */
const Track = ({
  guess,
  loading,
  track,
  layout,
  className,
}: {
  guess: TrackGuess | null;
  loading: boolean;
  /** What the server calls it, and which layout — the two rows the old "Running now" list
   *  carried, brought up beside the have-it-or-not line they belong with. */
  track: string;
  layout: string;
  className?: string;
}) => {
  const t = useT();
  if (loading) {
    return (
      <p className="flex items-center gap-2 text-[12.5px] text-faint">
        <Loader2 className="size-3.5 animate-spin" />
        {t("serverBrowser.trackChecking")}
      </p>
    );
  }
  return (
    <section className={cn("space-y-2", className)}>
      <h3 className="font-cond text-[11px] font-bold uppercase tracking-[0.14em] text-faint">
        {t("serverBrowser.trackTitle")}
      </h3>
      <div className="min-w-0 space-y-1">
        {/* The name IS the link. It was rendered three times over — once as the title, once
            inside "You have this track — X", and once more on a button that opened the page
            the title now opens. One name, one place to click. */}
        <p className="min-w-0 text-[15px] font-semibold tracking-[-0.01em]">
          {guess?.productUrl ? (
            <LinkedName href={guess.productUrl}>{track || "—"}</LinkedName>
          ) : (
            <span className="break-words">{track || "—"}</span>
          )}
          {layout ? <span className="text-muted-foreground"> · {layout}</span> : null}
        </p>

        {/* One short line about where you stand with it, and the identified name only when
            it is not the name above. */}
        {guess?.installed ? (
          <p className="flex items-center gap-1.5 text-[12.5px] text-muted-foreground">
            <CheckCircle2 className="size-3.5 shrink-0 text-faint" />
            <span className="truncate">
              {guess.stock ? t("serverBrowser.trackIsStock") : t("serverBrowser.trackIsYours")}
              {guess.installed && guess.installed !== track ? ` — ${guess.installed}` : ""}
            </span>
          </p>
        ) : guess?.productName ? (
          <p className="flex items-center gap-1.5 text-[12.5px] text-muted-foreground">
            <MapPin className="size-3.5 shrink-0 text-faint" />
            <span className="truncate">
              {guess.productName === track
                ? t("serverBrowser.trackFrom", { where: sourceName(guess.source, t) })
                : t("serverBrowser.trackMaybe", { name: guess.productName })}
            </span>
          </p>
        ) : null}
      </div>
    </section>
  );
};

/** Which catalogue a guess came from, in words. */
const sourceName = (source: string | null | undefined, t: ReturnType<typeof useT>): string =>
  source === "mods"
    ? t("serverBrowser.trackGetMods")
    : source === "hub"
      ? t("serverBrowser.trackGetHub")
      : t("serverBrowser.trackGetShop");

/**
 * A name that goes somewhere.
 *
 * Faded underline at rest, so it reads as text with somewhere to go rather than as a
 * control; the arrow only appears under the pointer, where it answers "where would this
 * take me" without shouting it on every row of every server.
 */
const LinkedName = ({ href, children }: { href: string; children: ReactNode }) => (
  <button
    type="button"
    onClick={() => void openUrl(href)}
    title={href}
    className="group inline-flex min-w-0 max-w-full cursor-default items-center gap-1 text-left"
  >
    <span className="truncate underline decoration-foreground/25 underline-offset-[3px] transition-colors group-hover:decoration-foreground/60">
      {children}
    </span>
    <ExternalLink className="size-3 flex-none opacity-0 transition-opacity group-hover:opacity-70" />
  </button>
);

/** One figure from the strip under the hero: a quiet label over the value that matters. */
const Stat = ({
  icon,
  label,
  value,
  tone,
}: {
  icon?: React.ReactNode;
  label: string;
  value: string;
  tone?: string;
}) => (
  <div className="min-w-0">
    <div className="font-cond text-[10px] font-bold uppercase tracking-[0.14em] text-faint">
      {label}
    </div>
    <div
      className={cn(
        "mt-0.5 flex items-center gap-1.5 font-cond text-[13px] font-semibold tabular-nums",
        tone,
      )}
    >
      {icon}
      <span className="truncate">{value}</span>
    </div>
  </div>
);

export interface ServerDetailProps {
  server: MasterServer | null;
  /** The track's preview, when the player has the track. */
  art?: string;
  /** The player doesn't have this track. False until that's known. */
  missing: boolean;
  /** Where a missing track comes from, when our server knows. */
  product?: CatalogTrack;
  /** Its track is installing, to join once it lands. */
  installing: boolean;
  favourite: boolean;
  /** The address a join is starting for, app-wide. */
  joining: string | null;
  /** Joining anything is blocked while another join is starting. */
  busy: boolean;
  queue: QueueState | null;
  onJoin: (address: string) => void;
  onWait: (server: MasterServer) => void;
  onInstallJoin: (s: MasterServer, product: CatalogTrack) => void;
  onCopy: (address: string) => void;
  onToggleFavourite: (address: string) => void;
  className?: string;
}

const ServerDetail = ({
  server,
  art,
  missing,
  product,
  installing,
  favourite,
  joining,
  busy,
  queue,
  onJoin,
  onWait,
  onInstallJoin,
  onCopy,
  onToggleFavourite,
  className,
}: ServerDetailProps) => {
  const { t, resolved } = useI18n();
  // What the row carried, replaced by the server's own answer once it arrives. Held here
  // rather than pushed back into the list: the list refreshes on its own schedule, and one
  // row updating under a player's cursor while they read it would be worse than stale.
  const [live, setLive] = useState<MasterServer | null>(null);
  const [riders, setRiders] = useState<ServerRiders | null>(null);
  const [ridersLoading, setRidersLoading] = useState(false);
  const [guess, setGuess] = useState<TrackGuess | null>(null);
  const [guessing, setGuessing] = useState(false);
  const [waiting, setWaiting] = useState(0);

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
    setWaiting(0);
    setRidersLoading(true);
    // One key asked, so the one value back is this server's line.
    queueCounts([address])
      .then((c) => !cancelled && setWaiting(Object.values(c)[0] ?? 0))
      .catch(() => {});
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
    // Answered from the cache when it has been asked before. Identifying a track can cost
    // four catalogue searches — mxb-mods, two shop passes, then the Hub — and a rotation
    // brings the same handful of tracks back every few minutes, off every row that runs
    // them. One answer per track per run of the app is enough.
    const hit = GUESSES.get(track);
    if (hit) {
      setGuess(hit);
      setGuessing(false);
      return;
    }
    let cancelled = false;
    setGuessing(true);
    guessServerTrack(track)
      .then((g) => {
        GUESSES.set(track, g);
        if (!cancelled) setGuess(g);
      })
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

  // The player's own copy of the track wins, then what our server knows it looks like, then
  // whatever the identification turned up — one picture, as wide as the pane.
  const hero = art || (missing ? product?.image : null) || guess?.preview || guess?.productImage;

  // The same four-way decision the tile makes, so a server offers the same thing whichever
  // way it is being looked at.
  const full = isFull(s);
  const free = missing && product?.source === "mods" && !!product.slug;
  const sold = missing && product?.source === "shop" ? product : null;
  const price = sold?.price;
  const priceLabel = !price
    ? ""
    : price.free
      ? t("shopCatalog.free")
      : formatPrice(price.sale ?? price.base, price.currency ?? "", resolved);

  return (
    <div className={cn("flex min-h-0 flex-col", className)}>
      <div className="relative aspect-[16/6] min-h-[132px] w-full shrink-0 overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
        {hero ? (
          <img src={hero} alt="" decoding="async" className="size-full object-cover" />
        ) : (
          <div className="grid size-full place-items-center text-foreground/20">
            <Mountain className="size-10" strokeWidth={1.5} />
          </div>
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-black/90 via-black/35 to-black/45" />
        <div className="absolute left-4 right-12 top-3 flex flex-wrap justify-end gap-1.5">
          {s.hidden && (
            <Badge variant="count" title={t("serverBrowser.hiddenBecause", { reason: s.hidden })}>
              {t("serverBrowser.filtered")}
            </Badge>
          )}
        </div>
        <div className="absolute inset-x-0 bottom-0 flex items-end gap-2 p-4">
          {s.passworded && (
            <Lock
              className="mb-1 size-4 shrink-0 text-white/70"
              aria-label={t("serverBrowser.passworded")}
            />
          )}
          <h2
            className="min-w-0 truncate font-cond text-[19px] font-bold tracking-[-0.01em] text-white"
            title={s.name}
          >
            {s.name || t("serverBrowser.unnamed")}
          </h2>
        </div>
      </div>

      {/* The four figures a join is decided on, in one band so two servers can be compared by
          looking at the same spot twice. Session and conditions used to sit here AND in a
          "Running now" list below; paint sync is a chip in the riders line when there is any,
          because a dash for it on every server is a column of nothing. */}
      <div className="grid shrink-0 grid-cols-4 gap-x-4 border-b border-input px-4 py-3">
        <Stat
          label={t("serverBrowser.players")}
          value={`${s.players}/${s.maxPlayers}`}
          icon={<Users className="size-3.5 text-faint" />}
        />
        <Stat
          label={t("serverBrowser.ping")}
          value={s.pingMs === null ? "—" : `${s.pingMs} ms`}
          icon={<Signal className="size-3.5 text-faint" />}
        />
        <Stat label={t("serverBrowser.session")} value={s.session || "—"} />
        <Stat label={t("serverBrowser.conditions")} value={s.conditions || "—"} />
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4">
        {/* What is being ridden and who is on it, on one line pushed apart: two short answers
            that used to be two stacked sections with a heading each. */}
        <div className="flex items-start justify-between gap-8">
          <Track
            guess={guess}
            loading={guessing}
            track={s.track}
            layout={s.trackLayout}
            className="min-w-0 flex-1"
          />
          <Riders
            riders={riders}
            loading={ridersLoading}
            className="max-w-[45%] flex-none text-right"
          />
        </div>

        {/* Rules and conditions in one two-column list. They were three stacked sections of
            single pairs, which is the same content at three times the height — and the last
            of them repeated the track, the session and the password that are already above. */}
        <Facts
          columns={2}
          title={t("serverBrowser.rules")}
          facts={[
            { label: t("serverBrowser.bikes"), value: list(s.bikes) },
            { label: t("serverBrowser.categories"), value: list(s.categories) },
            { label: t("serverBrowser.rating"), value: s.rating },
            { label: t("serverBrowser.raceLength"), value: s.raceLength },
            { label: t("serverBrowser.changingWeather"), value: flag(s.realisticWeather) },
            { label: t("serverBrowser.forceCockpit"), value: flag(s.forceCockpit) },
            { label: t("serverBrowser.noAids"), value: flag(s.noAids) },
            { label: t("serverBrowser.limitedTyres"), value: flag(s.limitedTyreSets) },
          ]}
        />

        {/* Not a section of its own: an address is for pasting once, not for reading. It sits
            on the rules block's own line, labelled like every pair above it, so it lands in
            the column the eye is already following rather than floating loose at the end. */}
        <dl className="grid grid-cols-[minmax(0,7rem)_1fr] gap-x-4 gap-y-1.5 border-t border-input pt-3 text-[13px]">
          <dt className="truncate text-muted-foreground">{t("serverBrowser.address")}</dt>
          <dd className="select-text min-w-0 break-all font-mono text-[12px]">
            {s.address}
            {s.lanAddress ? <span className="text-faint"> · {s.lanAddress}</span> : null}
          </dd>
        </dl>
      </div>

      <div className="shrink-0 space-y-2 border-t border-input px-4 py-3">
        {/* A server the game can't be pointed at says so, rather than offering a button
            that fails every time. */}
        {(!s.joinable || waiting > 0 || full) && (
          <p className="text-[12px] text-faint">
            {!s.joinable
              ? t("serverBrowser.notJoinable")
              : waiting > 0
                ? t("serverBrowser.waitingCount", { count: waiting })
                : t("serverBrowser.queueHint")}
          </p>
        )}
        <div className="flex items-center gap-2">
          {queue?.address === s.address ? (
            <Button variant="outline" className="flex-1" disabled>
              <Hourglass className="size-3.5" />
              {t("serverBrowser.inLine", { position: queue.position })}
            </Button>
          ) : installing ? (
            <Button variant="outline" className="flex-1" disabled>
              <Loader2 className="size-3.5 animate-spin" />
              {t("serverBrowser.installing")}
            </Button>
          ) : free && s.joinable && product ? (
            <Button
              className="flex-1"
              disabled={busy}
              onClick={() => onInstallJoin(s, product)}
              title={t("serverBrowser.installJoinHint", { title: product.name })}
            >
              <Download className="size-3.5" />
              {t("serverBrowser.installJoin")}
            </Button>
          ) : sold ? (
            <Button
              className="flex-1"
              onClick={() => void openShopUrl(sold.url)}
              title={t("serverBrowser.buyHint", { name: sold.name })}
            >
              <ShoppingCart className="size-3.5" />
              {priceLabel
                ? `${t("serverBrowser.buyTrack")} · ${priceLabel}`
                : t("serverBrowser.buyTrack")}
            </Button>
          ) : s.joinable && full ? (
            <Button
              variant="outline"
              className="flex-1"
              onClick={() => onWait(s)}
              title={t("serverBrowser.queueHint")}
            >
              <Hourglass className="size-3.5" />
              {t("serverBrowser.waitInLine")}
            </Button>
          ) : (
            <Button
              className="flex-1"
              onClick={() => onJoin(s.address)}
              disabled={!s.joinable || busy}
              title={s.joinable ? t("serverBrowser.join") : t("serverBrowser.notJoinable")}
            >
              {joining === s.address ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Plug className="size-3.5" />
              )}
              {t("serverBrowser.join")}
            </Button>
          )}
          <Button
            variant="outline"
            className="shrink-0 px-3"
            onClick={() => onCopy(s.address)}
            title={t("serverBrowser.copyAddress")}
            aria-label={t("serverBrowser.copyAddress")}
          >
            <Copy className="size-3.5" />
          </Button>
          <Button
            variant="outline"
            className={cn("shrink-0 px-3", favourite && "text-amber-300")}
            onClick={() => onToggleFavourite(s.address)}
            title={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
            aria-label={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
            aria-pressed={favourite}
          >
            <Star className="size-3.5" fill={favourite ? "currentColor" : "none"} />
          </Button>
        </div>
      </div>
    </div>
  );
};

/** Nothing picked yet: the pane says what it is for rather than sitting blank. */
export const ServerDetailEmpty = () => {
  const t = useT();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
      <ServerOff className="size-6 text-faint" />
      <p className="text-[13px] text-faint">{t("serverBrowser.pickServer")}</p>
    </div>
  );
};

/** The tiles still open the pane over the grid — one tile offers one server, and there is no
 *  list beside it to put the pane next to. */
export const ServerDetailDialog = ({
  onOpenChange,
  ...pane
}: ServerDetailProps & { onOpenChange: (open: boolean) => void }) => (
  <Dialog open={!!pane.server} onOpenChange={onOpenChange}>
    <DialogContent className="max-w-[620px] gap-0 overflow-hidden p-0">
      <DialogTitle className="sr-only">{pane.server?.name ?? ""}</DialogTitle>
      <ServerDetail {...pane} className="max-h-[82vh]" />
    </DialogContent>
  </Dialog>
);

export default ServerDetail;
