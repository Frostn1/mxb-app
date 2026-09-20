import { memo } from "react";
import {
  Mountain,
  Users,
  Lock,
  Play,
  Loader2,
  Wifi,
  Copy,
  Star,
  Hourglass,
  Palette,
  Download,
  ShoppingCart,
} from "lucide-react";
import type { CatalogTrack, MasterServer } from "@frost/shared/api/mods";
import { Badge } from "@frost/shared/Components/ui/badge";
import { cn } from "@frost/shared/lib/utils";
import { useI18n } from "@/i18n";
import { formatPrice, openShopUrl } from "../../api/shop";
import { isFull } from "@/lib/useServerQueue";

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
  favourite: boolean;
  /** Riders on this server running paint sync. */
  paintSync: number;
  joining: boolean;
  /** Joining anything is blocked while another join is starting. */
  busy: boolean;
  /** The player's place, when they're in line for this server. */
  queuePosition: number | null;
  onOpen: (s: MasterServer) => void;
  onJoin: (address: string) => void;
  onWait: (s: MasterServer) => void;
  onCopy: (address: string) => void;
  onToggleFavourite: (address: string) => void;
}

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
}: Props) {
  const { t, resolved } = useI18n();
  const full = isFull(s);
  const cat = s.categories[0];
  const stop = (e: React.MouseEvent) => e.stopPropagation();
  // The player's own copy wins; a missing track shows what it looks like, from our server.
  // The player's own copy wins; a track they do NOT have shows what it looks like, from
  // the store. Never the other way round — that is a guess at a name match.
  const picture = art || (missing ? product?.image : null);
  const free = missing && product?.source === "mods" && !!product.slug;
  const sold = missing && product?.source === "shop" ? product : null;
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
        {picture ? (
          <img
            src={picture}
            alt={s.track}
            decoding="async"
            loading="lazy"
            className="size-full object-cover"
          />
        ) : (
          <div className="grid size-full place-items-center text-foreground/20">
            <Mountain className="size-8" strokeWidth={1.5} />
          </div>
        )}

        <span
          className={cn(
            "absolute right-1.5 top-1.5 flex items-center gap-1.5 rounded-md bg-black/70 px-2 py-1 text-[15px] font-bold tabular-nums shadow-sm backdrop-blur-[2px]",
            s.players > 0 ? "text-white" : "text-white/70",
          )}
        >
          <Users className="size-3.5" strokeWidth={2.5} />
          {s.players}/{s.maxPlayers}
        </span>

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

        {paintSync > 0 && (
          <span
            className="absolute bottom-1.5 right-1.5 flex items-center gap-1 rounded-md bg-black/70 px-1.5 py-[3px] text-[12px] font-bold tabular-nums text-success shadow-sm"
            title={t("serverBrowser.paintSyncHere", { count: paintSync })}
          >
            <Palette className="size-3" />
            {paintSync}
          </span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-1.5 px-3 py-2.5">
        <span className="truncate text-[13px] font-semibold" title={s.name}>
          {s.name || t("serverBrowser.unnamed")}
        </span>
        <div className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
          <span className="truncate" title={[s.track, s.trackLayout].filter(Boolean).join(" · ")}>
            {s.track || "-"}
          </span>
          {cat && (
            <>
              <span className="opacity-40">·</span>
              <span className="truncate">{cat}</span>
            </>
          )}
          {/* A track you don't have is an errand, not a fault. It read as one while this was
              an amber hazard sign across the picture — the tile looked broken, and the button
              under it already says what to do about it. On the track's own line, so it says
              which thing is missing and costs the tile no height. */}
          {missing && (
            <Badge
              variant="count"
              className="ml-auto shrink-0"
              title={t("serverBrowser.trackMissing")}
            >
              <Download className="size-3" />
              {t("serverBrowser.notInstalled")}
            </Badge>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1">
          {s.hidden && (
            <Badge variant="count" title={t("serverBrowser.hiddenBecause", { reason: s.hidden })}>
              {t("serverBrowser.filtered")}
            </Badge>
          )}
          {s.location && s.location !== "?" && (
            <Badge variant="count" className="font-medium">
              {s.location}
            </Badge>
          )}
          {s.session && <Badge variant="count">{s.session}</Badge>}
          {s.conditions && s.conditions !== "?" && <Badge variant="count">{s.conditions}</Badge>}
        </div>

        <div className="mt-auto flex items-center gap-2 pt-1.5">
          {queuePosition !== null ? (
            <CardButton disabled>
              <Hourglass className="size-3.5" />
              {t("serverBrowser.inLine", { position: queuePosition })}
            </CardButton>
          ) : installing ? (
            <CardButton disabled>
              <Loader2 className="size-3.5 animate-spin" />
              {t("serverBrowser.installing")}
            </CardButton>
          ) : free && product ? (
            <>
              <CardButton
                primary={!s.joinable}
                className="flex-none"
                onClick={(e) => {
                  stop(e);
                  onInstall(s, product);
                }}
                title={t("serverBrowser.installHint", { title: product.name })}
              >
                {t("serverBrowser.install")}
              </CardButton>
              {s.joinable && (
                <CardButton
                  primary
                  disabled={busy}
                  onClick={(e) => {
                    stop(e);
                    onInstallJoin(s, product);
                  }}
                  title={t("serverBrowser.installJoinHint", { title: product.name })}
                >
                  {t("serverBrowser.installJoin")}
                </CardButton>
              )}
            </>
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
          ) : s.joinable && full ? (
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
                onJoin(s.address);
              }}
              title={s.joinable ? t("serverBrowser.join") : t("serverBrowser.notJoinable")}
            >
              {joining ? <Loader2 className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
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

export default ServerCard;
