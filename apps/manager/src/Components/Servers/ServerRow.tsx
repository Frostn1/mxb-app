import { memo } from "react";
import { Lock, Users, Wifi, Palette, Star, Mountain, Hourglass, Download } from "lucide-react";
import type { CatalogTrack, MasterServer } from "@frost/shared/api/mods";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

/** Latency to colour, the steps the tile uses. */
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
  /** What the store knows about a track the player lacks — its picture, mostly. The tile
   *  has always fallen back to this; a row that did not showed an empty slot beside a tile
   *  showing the artwork. */
  product?: CatalogTrack;
  /** This is the row the pane beside the list is showing. */
  selected: boolean;
  favourite: boolean;
  /** Riders on this server running paint sync. */
  paintSync: number;
  /** The player's place, when they're in line for this server. */
  queuePosition: number | null;
  onSelect: (s: MasterServer) => void;
  onToggleFavourite: (address: string) => void;
}

/**
 * One server as a row in the master list: the art small, the name, and the two figures
 * anybody sorts on. Everything else about the server is one click away in the pane beside
 * it, which is the whole point of the row being this narrow.
 *
 * Memoised on the server object the same way the tile is, so a sweep that moves four rider
 * counts redraws four rows.
 */
const ServerRow = memo(function ServerRow({
  server: s,
  art,
  missing,
  product,
  selected,
  favourite,
  paintSync,
  queuePosition,
  onSelect,
  onToggleFavourite,
}: Props) {
  const t = useT();
  // The player's own copy wins; a track they lack shows what it looks like, from the store.
  // Cache and the library first, the store only when neither had one — a track the player
  // has but whose file carries no preview still gets a picture.
  const picture = art || product?.image || null;

  return (
    <div
      onClick={() => onSelect(s)}
      aria-current={selected ? "true" : undefined}
      className={cn(
        "relative flex cursor-pointer items-center gap-2.5 border-b border-input/60 px-3 py-2 transition-colors last:border-0",
        selected ? "bg-accent" : "hover:bg-foreground/[0.04]",
        // Revealed rows stay legible but visibly demoted, so nobody mistakes one for an
        // ordinary result they just hadn't scrolled to.
        s.hidden && !selected && "opacity-55",
      )}
    >
      {selected && <span className="absolute inset-y-0 left-0 w-[3px] bg-primary" />}

      <div className="relative h-8 w-[52px] shrink-0 overflow-hidden rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a]">
        {picture ? (
          <img
            src={picture}
            alt=""
            decoding="async"
            loading="lazy"
            className="size-full object-cover"
          />
        ) : (
          <div className="grid size-full place-items-center text-foreground/20">
            <Mountain className="size-3.5" strokeWidth={1.5} />
          </div>
        )}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          {s.passworded && (
            <Lock
              className="size-3 shrink-0 text-faint"
              aria-label={t("serverBrowser.passworded")}
            />
          )}
          <span className="truncate text-[12.5px] font-semibold" title={s.name}>
            {s.name || t("serverBrowser.unnamed")}
          </span>
          {queuePosition !== null && (
            <Hourglass
              className="size-3 shrink-0 text-primary"
              aria-label={t("serverBrowser.inLine", { position: queuePosition })}
            />
          )}
          {paintSync > 0 && (
            <span
              className="inline-flex shrink-0 items-center gap-0.5 font-cond text-[10.5px] font-bold tabular-nums text-success"
              title={t("serverBrowser.paintSyncHere", { count: paintSync })}
            >
              <Palette className="size-3" />
              {paintSync}
            </span>
          )}
          {missing && (
            <Download
              className="size-3 shrink-0 text-faint"
              aria-label={t("serverBrowser.trackMissing")}
            />
          )}
        </div>
        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <span
            className="truncate"
            title={[s.track, s.trackLayout].filter(Boolean).join(" · ")}
          >
            {s.track || "—"}
          </span>
          {s.location && s.location !== "?" && (
            <>
              <span className="shrink-0 opacity-40">·</span>
              <span className="shrink-0 truncate">{s.location}</span>
            </>
          )}
          {s.hidden && (
            <span
              className="shrink-0 font-cond text-[10px] uppercase tracking-[0.14em] text-faint"
              title={t("serverBrowser.hiddenBecause", { reason: s.hidden })}
            >
              {t("serverBrowser.filtered")}
            </span>
          )}
        </div>
      </div>

      <div className="flex shrink-0 flex-col items-end gap-0.5 font-cond text-[11.5px] font-semibold tabular-nums">
        <span className="flex items-center gap-1">
          <Users className="size-3 text-faint" />
          {s.players}/{s.maxPlayers}
        </span>
        {s.pingMs === null ? (
          <span className="text-faint">—</span>
        ) : (
          <span
            className={cn("flex items-center gap-1", pingTone(s.pingMs))}
            title={t("serverBrowser.ping.title", { ms: s.pingMs })}
          >
            <Wifi className="size-3" />
            {s.pingMs}
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onToggleFavourite(s.address);
        }}
        title={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
        aria-label={favourite ? t("serverBrowser.unstar") : t("serverBrowser.star")}
        aria-pressed={favourite}
        className={cn(
          "grid size-6 shrink-0 cursor-default place-items-center rounded-md transition-colors",
          favourite ? "text-amber-300" : "text-faint hover:text-muted-foreground",
        )}
      >
        <Star className="size-3.5" fill={favourite ? "currentColor" : "none"} />
      </button>
    </div>
  );
});

export default ServerRow;
