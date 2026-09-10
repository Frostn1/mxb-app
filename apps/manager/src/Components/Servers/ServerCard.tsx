import { useEffect, useState } from "react";
import { Mountain, Users, Lock, Play, AlertTriangle, Loader2, Wifi, Copy, Star } from "lucide-react";
import { toast } from "sonner";
import { getPkzPreview, type MasterServer, type TrackHit } from "@frost/shared/api/mods";
import type { TrackMatch } from "@/lib/trackContent";
import { Badge } from "@frost/shared/Components/ui/badge";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { regionLabel } from "@/lib/serverRegion";
import TrackContentAction from "./TrackContentAction";

/** Latency → colour, so the figure reads at a glance: close is green, far is red. */
function pingTone(rtt: number | null): string {
  if (rtt === null) return "text-white/60";
  if (rtt < 60) return "text-emerald-300";
  if (rtt < 120) return "text-lime-300";
  if (rtt < 200) return "text-amber-300";
  return "text-red-300";
}

interface Props {
  server: MasterServer;
  match: TrackMatch;
  /** The catalog post that has this server's track, when the index already found one. */
  known?: TrackHit;
  /** The index is still looking for this track. */
  pending?: boolean;
  joining: boolean;
  onJoin: () => void;
  onOpenHub?: () => void;
  /** Starred by the player: the star is filled and the card can be found under Favourites. */
  favourite?: boolean;
  onToggleFavourite?: () => void;
}

/**
 * One server as a card: the track's own preview art where we have it, an amber "missing
 * content" flag over a placeholder where we don't, and the live stats (players, region,
 * category, session state) below. Joining is one click; a missing track offers to fetch itself.
 *
 * Ported from the standalone v0.13 browser and mapped onto upstream's `MasterServer` shape
 * (`passworded`, `location`, `categories`, `session`, `conditions`, `pingMs`).
 */
export default function ServerCard({
  server,
  match,
  known,
  pending,
  joining,
  onJoin,
  onOpenHub,
  favourite = false,
  onToggleFavourite,
}: Props) {
  const t = useT();
  const [thumb, setThumb] = useState<string | null>(null);

  // The archive to read art out of, or null when there is nothing installed to read. A plain
  // string, deliberately: `matchTrack` builds a fresh object every render, so keying the
  // effect on `match` itself would re-run it - an IPC call per card, per render. The path is
  // stable, so the preview is fetched once per track and survives every refresh.
  const previewPath = match.state === "installed" ? (match.installed?.path ?? null) : null;

  useEffect(() => {
    if (!previewPath) {
      setThumb(null);
      return;
    }
    let live = true;
    getPkzPreview(previewPath)
      .then((p) => live && setThumb(p))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [previewPath]);

  const missing = match.state === "missing";
  const full = server.maxPlayers > 0 && server.players >= server.maxPlayers;
  const cat = server.categories[0];

  return (
    <div
      className={cn(
        "group relative flex flex-col overflow-hidden rounded-xl border bg-card text-left transition-colors",
        "border-white/[0.07] hover:border-white/15",
      )}
    >
      <div className="relative aspect-video overflow-hidden bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
        {thumb ? (
          <img src={thumb} alt={server.track} className="size-full object-cover" />
        ) : (
          <div className="grid size-full place-items-center text-foreground/20">
            <Mountain className="size-8" strokeWidth={1.5} />
          </div>
        )}

        {/* Missing-content flag, over the image the way the design overlays a badge. */}
        {missing && (
          <div className="absolute inset-0 flex items-center justify-center bg-black/45 backdrop-blur-[1px]">
            <span className="flex items-center gap-1.5 rounded-md bg-amber-500/90 px-2 py-1 text-[11px] font-bold text-black shadow">
              <AlertTriangle className="size-3.5" strokeWidth={2.5} />
              {t("serverBrowser.missingContent")}
            </span>
          </div>
        )}

        {/* Player count, top-right, like the rating pill on a mod card. */}
        <span
          className={cn(
            "absolute right-1.5 top-1.5 flex items-center gap-1.5 rounded-md bg-black/70 px-2 py-1 text-[15px] font-bold tabular-nums shadow-sm backdrop-blur-[2px]",
            server.players > 0 ? "text-white" : "text-white/70",
          )}
        >
          <Users className="size-3.5" strokeWidth={2.5} />
          {server.players}/{server.maxPlayers}
        </span>

        {/* Star and lock share the top-left corner. The star is always rendered, not revealed
            on hover: a card that shows nothing until the pointer arrives gives the player no
            way to learn the feature exists. */}
        <span className="absolute left-1.5 top-1.5 flex items-center gap-1">
          {onToggleFavourite && (
            <button
              onClick={onToggleFavourite}
              title={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
              aria-label={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
              aria-pressed={favourite}
              className={cn(
                "grid size-5 place-items-center rounded-md bg-black/65 shadow-sm transition-colors cursor-default",
                favourite ? "text-amber-300" : "text-white/70 hover:text-white",
              )}
            >
              <Star className="size-3" fill={favourite ? "currentColor" : "none"} />
            </button>
          )}
          {server.passworded && (
            <span className="grid size-5 place-items-center rounded-md bg-black/65 text-white/80 shadow-sm">
              <Lock className="size-3" />
            </span>
          )}
        </span>

        {/* Latency, bottom-left - the other number people pick a server on. */}
        {server.pingMs !== null && (
          <span
            className={cn(
              "absolute bottom-1.5 left-1.5 flex items-center gap-1 rounded-md bg-black/70 px-1.5 py-[3px] text-[12.5px] font-bold tabular-nums shadow-sm backdrop-blur-[2px]",
              pingTone(server.pingMs),
            )}
            title={t("serverBrowser.ping.title", { ms: server.pingMs })}
          >
            <Wifi className="size-3" strokeWidth={2.5} />
            {t("serverBrowser.ping.ms", { ms: server.pingMs })}
          </span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-1.5 px-3 py-2.5">
        <span className="truncate text-[13px] font-semibold" title={server.name}>
          {server.name || t("serverBrowser.unnamed")}
        </span>
        <div className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
          <span className="truncate" title={server.track}>
            {server.track || "-"}
          </span>
          {cat && (
            <>
              <span className="opacity-40">·</span>
              <span className="truncate">{cat}</span>
            </>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1">
          {regionLabel(server.location) && (
            <Badge variant="count" className="font-medium">
              {regionLabel(server.location)}
            </Badge>
          )}
          {server.session && <Badge variant="count">{server.session}</Badge>}
          {server.conditions && server.conditions !== "?" && (
            <Badge variant="count">{server.conditions}</Badge>
          )}
        </div>

        <div className="mt-auto flex items-center gap-2 pt-1.5">
          <button
            onClick={onJoin}
            disabled={joining || !server.joinable}
            className={cn(
              "inline-flex flex-1 items-center justify-center gap-1.5 rounded-md px-2.5 py-1.5 text-[12px] font-semibold transition-colors cursor-default",
              full || !server.joinable
                ? "border border-white/[0.1] text-muted-foreground"
                : "bg-secondary text-foreground hover:brightness-110",
            )}
            title={
              !server.joinable
                ? t("serverBrowser.notJoinable")
                : full
                  ? t("serverBrowser.full")
                  : t("serverBrowser.join")
            }
          >
            {joining ? <Loader2 className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
            {t("serverBrowser.join")}
          </button>
          {/* The `ip:port` the master gave us - the one thing someone needs to hand a server
              to a friend, or to reconnect from the game's own address box. */}
          <button
            onClick={() => {
              navigator.clipboard
                .writeText(server.address)
                .then(() => toast.success(t("serverBrowser.copied")))
                .catch(() => toast.error(t("serverBrowser.copyFailed")));
            }}
            title={t("serverBrowser.copyAddress")}
            aria-label={t("serverBrowser.copyAddress")}
            className="grid size-[30px] shrink-0 place-items-center rounded-md border border-white/[0.1] text-muted-foreground transition-colors cursor-default hover:text-foreground"
          >
            <Copy className="size-3.5" />
          </button>
          {missing && (
            <TrackContentAction
              track={server.track}
              known={known}
              pending={pending}
              onOpenHub={onOpenHub}
              compact
            />
          )}
        </div>
      </div>
    </div>
  );
}
