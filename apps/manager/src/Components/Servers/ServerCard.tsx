import { memo, useState } from "react";
import {
  Mountain,
  Users,
  Lock,
  Loader2,
  Wifi,
  WifiOff,
  Copy,
  Star,
  Hourglass,
  Palette,
  Download,
  ShoppingCart,
  Power,
} from "lucide-react";
import type { CatalogTrack, MasterServer } from "@frost/shared/api/mods";
import { Badge } from "@frost/shared/Components/ui/badge";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import { GRID_THUMB_WIDTH, isCacheableImage } from "@frost/shared/lib/imgcache";
import { cn } from "@frost/shared/lib/utils";
import { useI18n } from "@/i18n";
import { formatPrice, openShopUrl } from "../../api/shop";
import { isFull } from "@/lib/useServerQueue";
import { joinAction } from "./joinAction";

/** Latency to colour: close is green, far is red. */
function pingTone(ms: number): string {
  if (ms < 60) return "text-emerald-300";
  if (ms < 120) return "text-lime-300";
  if (ms < 200) return "text-amber-300";
  return "text-red-300";
}

interface Props {
  server: MasterServer;
  /** The track's preview, when the player has the track. */
  art?: string;
  /** The player doesn't have this track. False until that's known. */
  missing: boolean;
  /** Where a missing track comes from, when our server knows. */
  product?: CatalogTrack;
  /** Its track is installing. */
  installing: boolean;
  onInstall: (s: MasterServer, product: CatalogTrack) => void;
  onInstallJoin: (s: MasterServer, product: CatalogTrack) => void;
  /** The `rel` of the player's own copy, when Manage has switched the track off. */
  inactive?: string;
  /** Switch a parked track back on, then join. */
  onActivateJoin: (s: MasterServer, rel: string) => void;
  favourite: boolean;
  /** Riders on this server running paint sync. */
  paintSync: number;
  joining: boolean;
  /** Joining anything is blocked while another join is starting. */
  busy: boolean;
  /** The player's place, when they're in line for this server. */
  queuePosition: number | null;
  onOpen: (s: MasterServer) => void;
  onJoin: (server: MasterServer) => void;
  onWait: (s: MasterServer) => void;
  onCopy: (address: string) => void;
  onToggleFavourite: (address: string) => void;
  /** A saved server's reachability, asked of it directly. Absent for a row off the master
   *  list, which answered by being on it. */
  status?: SavedStatus;
  /** Extra controls after Copy — the Saved row's edit / move / remove menu. */
  menu?: React.ReactNode;
}

export type SavedStatus = "online" | "offline" | "checking";

/**
 * One server as a tile: the track's own art, the live numbers over it, and a join button.
 * Memoised, so a list refresh redraws only the tiles whose server changed.
 */
const ServerCard = memo(function ServerCard({
  server: s,
  art,
  missing,
  product,
  installing,
  onInstall,
  onInstallJoin,
  inactive,
  onActivateJoin,
  favourite,
  paintSync,
  joining,
  busy,
  queuePosition,
  onOpen,
  onJoin,
  onWait,
  onCopy,
  onToggleFavourite,
  status,
  menu,
}: Props) {
  const { t, resolved } = useI18n();
  const full = isFull(s);
  // A server that didn't answer has no rider count to show; "0/0" would read as an answer.
  const answered = status === undefined || status === "online";
  const cat = s.categories[0];
  const stop = (e: React.MouseEvent) => e.stopPropagation();
  // The player's own copy wins; a missing track shows what it looks like, from our server.
  // The player's own copy wins; a track they do NOT have shows what it looks like, from
  // the store. Never the other way round — that is a guess at a name match.
  const picture = art || (missing ? product?.image : null);
  const [unavailablePicture, setUnavailablePicture] = useState<string | null>(null);
  const shownPicture = picture === unavailablePicture ? null : picture;
  const action = joinAction({
    missing,
    inactive,
    product,
    installing,
    queued: queuePosition !== null,
    joinable: s.joinable,
    full,
  });
  const sold = action.kind === "buy" ? action.product : null;
  const price = sold?.price;
  const priceLabel = !price
    ? ""
    : price.free
      ? t("shopCatalog.free")
      : formatPrice(price.sale ?? price.base, price.currency ?? "", resolved);

  return (
    <div
      onClick={() => onOpen(s)}
      className={cn(
        "group relative flex cursor-pointer flex-col overflow-hidden rounded-xl border border-white/[0.07] bg-card transition-colors hover:border-white/15",
        // Off-screen tiles skip layout and paint until scrolled to.
        "[contain-intrinsic-size:auto_300px] [content-visibility:auto]",
        s.hidden && "opacity-55",
      )}
    >
      <div className="relative aspect-video overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
        {shownPicture ? (
          isCacheableImage(shownPicture) ? (
            <CachedImg
              src={shownPicture}
              width={GRID_THUMB_WIDTH}
              alt={s.track}
              decoding="async"
              loading="lazy"
              onUnavailable={() => setUnavailablePicture(shownPicture)}
              className="size-full object-cover"
            />
          ) : (
            <img
              src={shownPicture}
              alt={s.track}
              decoding="async"
              loading="lazy"
              onError={() => setUnavailablePicture(shownPicture)}
              className="size-full object-cover"
            />
          )
        ) : (
          <div className="grid size-full place-items-center text-foreground/20">
            <Mountain className="size-8" strokeWidth={1.5} />
          </div>
        )}

        {answered ? (
          <span
            className={cn(
              "absolute right-1.5 top-1.5 flex items-center gap-1.5 rounded-md bg-black/70 px-2 py-1 text-[15px] font-bold tabular-nums shadow-sm backdrop-blur-[2px]",
              s.players > 0 ? "text-white" : "text-white/70",
            )}
          >
            <Users className="size-3.5" strokeWidth={2.5} />
            {s.players}/{s.maxPlayers}
          </span>
        ) : (
          <span
            className="absolute right-1.5 top-1.5 flex items-center gap-1.5 rounded-md bg-black/70 px-2 py-1 text-[12px] font-semibold text-white/70 shadow-sm backdrop-blur-[2px]"
            title={status === "offline" ? t("savedServers.offlineHint") : undefined}
          >
            {status === "checking" ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <WifiOff className="size-3" />
            )}
            {status === "checking" ? t("savedServers.checking") : t("savedServers.offline")}
          </span>
        )}

        <span className="absolute left-1.5 top-1.5 flex items-center gap-1">
          <button
            type="button"
            onClick={(e) => {
              stop(e);
              onToggleFavourite(s.address);
            }}
            title={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
            aria-label={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
            aria-pressed={favourite}
            className={cn(
              "grid size-5 cursor-default place-items-center rounded-md bg-black/65 shadow-sm transition-colors",
              favourite ? "text-amber-300" : "text-white/70 hover:text-white",
            )}
          >
            <Star className="size-3" fill={favourite ? "currentColor" : "none"} />
          </button>
          {s.passworded && (
            <span
              className="grid size-5 place-items-center rounded-md bg-black/65 text-white/80 shadow-sm"
              aria-label={t("serverBrowser.passworded")}
            >
              <Lock className="size-3" />
            </span>
          )}
        </span>

        {s.pingMs !== null && (
          <span
            className={cn(
              "absolute bottom-1.5 left-1.5 flex items-center gap-1 rounded-md bg-black/70 px-1.5 py-[3px] text-[12.5px] font-bold tabular-nums shadow-sm backdrop-blur-[2px]",
              pingTone(s.pingMs),
            )}
            title={t("serverBrowser.ping.title", { ms: s.pingMs })}
          >
            <Wifi className="size-3" strokeWidth={2.5} />
            {s.pingMs} ms
          </span>
        )}

        {/* The fourth corner. "Not installed" is an errand, not a fault, so it sits faded on
            the picture with the other overlays rather than squeezing the track's own line. */}
        {(paintSync > 0 || (missing && !inactive)) && (
          <span className="absolute bottom-1.5 right-1.5 flex items-center gap-1">
            {missing && !inactive && (
              <span
                className="flex items-center gap-1 rounded-md bg-black/55 px-1.5 py-[3px] text-[11px] font-semibold uppercase tracking-wide text-white/75 shadow-sm backdrop-blur-[2px]"
                title={t("serverBrowser.trackMissing")}
              >
                <Download className="size-3" />
                {t("serverBrowser.notInstalled")}
              </span>
            )}
            {paintSync > 0 && (
              <span
                className="flex items-center gap-1 rounded-md bg-black/70 px-1.5 py-[3px] text-[12px] font-bold tabular-nums text-success shadow-sm"
                title={t("serverBrowser.paintSyncHere", { count: paintSync })}
              >
                <Palette className="size-3" />
                {paintSync}
              </span>
            )}
          </span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 px-3.5 py-3">
        <span className="truncate text-[13px] font-semibold" title={s.name}>
          {s.name || t("serverBrowser.unnamed")}
        </span>
        <div className="flex min-w-0 items-center gap-1.5 text-[11.5px] text-muted-foreground">
          <span className="truncate" title={[s.track, s.trackLayout].filter(Boolean).join(" · ")}>
            {s.track || "-"}
          </span>
          {cat && (
            <>
              <span className="opacity-40">·</span>
              <span className="truncate">{cat}</span>
            </>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          {s.hidden && (
            <Badge variant="count" title={t("serverBrowser.hiddenBecause", { reason: s.hidden })}>
              {t("serverBrowser.filtered")}
            </Badge>
          )}
          {known(s.location) && (
            <Badge variant="count" className="font-medium">
              {s.location}
            </Badge>
          )}
          {known(s.session) && <Badge variant="count">{s.session}</Badge>}
          {known(s.conditions) && <Badge variant="count">{s.conditions}</Badge>}
        </div>

        <div className="mt-auto flex flex-wrap items-center gap-2 pt-2">
          {action.kind === "queued" ? (
            <CardButton disabled>
              <Hourglass className="size-3.5" />
              {t("serverBrowser.inLine", { position: queuePosition ?? 0 })}
            </CardButton>
          ) : action.kind === "installing" ? (
            <CardButton disabled>
              <Loader2 className="size-3.5 animate-spin" />
              {t("serverBrowser.installing")}
            </CardButton>
          ) : action.kind === "activate" ? (
            <CardButton
              primary={s.joinable}
              disabled={busy || !s.joinable}
              onClick={(e) => {
                stop(e);
                onActivateJoin(s, action.rel);
              }}
              title={t("serverBrowser.activateJoinHint")}
            >
              {joining ? <Loader2 className="size-3.5 animate-spin" /> : <Power className="size-3.5" />}
              {t("serverBrowser.activateJoin")}
            </CardButton>
          ) : action.kind === "install" ? (
            <>
              <CardButton
                primary={!s.joinable}
                className="flex-none"
                onClick={(e) => {
                  stop(e);
                  onInstall(s, action.product);
                }}
                title={t("serverBrowser.installHint", { title: action.product.name })}
              >
                {t("serverBrowser.install")}
              </CardButton>
              {s.joinable && (
                <CardButton
                  primary
                  disabled={busy}
                  onClick={(e) => {
                    stop(e);
                    onInstallJoin(s, action.product);
                  }}
                  title={t("serverBrowser.installJoinHint", { title: action.product.name })}
                >
                  {t("serverBrowser.installJoin")}
                </CardButton>
              )}
            </>
          ) : action.kind === "missing" ? (
            // Not a plain Join: the player doesn't have this track, and a catalogue name that
            // only resembles the id is not a reason to pretend otherwise. Still possible — a
            // server may be about to rotate — but outlined and said plainly.
            <CardButton
              disabled={busy || !s.joinable}
              onClick={(e) => {
                stop(e);
                onJoin(s);
              }}
              title={t("serverBrowser.joinAnywayHint")}
            >
              {joining && <Loader2 className="size-3.5 animate-spin" />}
              {t("serverBrowser.joinAnyway")}
            </CardButton>
          ) : sold ? (
            <CardButton
              primary
              onClick={(e) => {
                stop(e);
                void openShopUrl(sold.url);
              }}
              title={t("serverBrowser.buyHint", { name: sold.name })}
            >
              <ShoppingCart className="size-3.5" />
              {priceLabel ? `${t("serverBrowser.buyTrack")} · ${priceLabel}` : t("serverBrowser.buyTrack")}
            </CardButton>
          ) : action.kind === "wait" ? (
            <CardButton
              onClick={(e) => {
                stop(e);
                onWait(s);
              }}
              title={t("serverBrowser.queueHint")}
            >
              <Hourglass className="size-3.5" />
              {t("serverBrowser.waitInLine")}
            </CardButton>
          ) : (
            <CardButton
              primary={s.joinable}
              disabled={busy || !s.joinable}
              onClick={(e) => {
                stop(e);
                onJoin(s);
              }}
              title={s.joinable ? t("serverBrowser.join") : t("serverBrowser.notJoinable")}
            >
              {joining && <Loader2 className="size-3.5 animate-spin" />}
              {t("serverBrowser.join")}
            </CardButton>
          )}
          <button
            type="button"
            onClick={(e) => {
              stop(e);
              onCopy(s.address);
            }}
            title={t("serverBrowser.copyAddress")}
            aria-label={t("serverBrowser.copyAddress")}
            className="grid size-[30px] shrink-0 cursor-default place-items-center rounded-md border border-white/[0.1] text-muted-foreground transition-colors hover:text-foreground"
          >
            <Copy className="size-3.5" />
          </button>
          {menu}
        </div>
      </div>
    </div>
  );
});

const CardButton = ({
  primary = false,
  className,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { primary?: boolean }) => (
  <button
    type="button"
    {...props}
    className={cn(
      "inline-flex flex-1 cursor-default items-center justify-center gap-1.5 rounded-md px-2.5 py-1.5 text-[12px] font-semibold transition-colors disabled:opacity-60",
      primary
        ? "bg-secondary text-foreground hover:brightness-110"
        : "border border-white/[0.1] text-muted-foreground",
      className,
    )}
  />
);

/** A server field worth a chip: the master reports what it doesn't know as `?`, blank or "Unknown". */
function known(v: string | null | undefined): v is string {
  const t = v?.trim();
  return !!t && t !== "?" && t.toLowerCase() !== "unknown";
}

export default ServerCard;
