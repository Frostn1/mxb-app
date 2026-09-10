import { useCallback, useState } from "react";
import { Download, Loader2, Tag, ExternalLink, Search } from "lucide-react";
import { toast } from "sonner";
import { useInstall } from "@/Context/Install";
import { useConfig } from "@frost/shared/Context/Config";
import { useI18n } from "@frost/shared/i18n/context";
import { useT } from "@/i18n";
import { resolveMissingTrack, trackQuery, type TrackSource } from "@/lib/trackDownload";
import { resolveQuickInstall, modTypesFor, type TrackHit } from "@frost/shared/api/mods";
import { openShopUrl } from "@/api/shop";
import { open } from "@tauri-apps/plugin-shell";
import { cn } from "@frost/shared/lib/utils";

interface Props {
  /** The server's track id (internal name). */
  track: string;
  /** The catalog post the backend's index already matched this track to, when it has one
   *  (see {@link useTrackCatalog}). Its presence is what turns this control from "look for
   *  it" into "install it" — no search, no click spent finding out. */
  known?: TrackHit;
  /** The index is still reading candidate pages for this track: neither found nor ruled out
   *  yet, so the control waits rather than offering a search that may be about to be
   *  answered for free. */
  pending?: boolean;
  /** Compact styling for the dense list view. */
  compact?: boolean;
  /** Open the Hub tab (for a Hub product the user should complete there). */
  onOpenHub?: () => void;
}

/**
 * The "this track isn't installed — get it" control.
 *
 * The best case costs nothing: when the track index has already matched this track to an
 * mxb-mods post ({@link Props.known}), the button installs it directly. That covers the
 * tracks servers actually run, including the ones no title match would find — a post titled
 * "Farm14 v0.1" ships `Farm14.pkz`, and the index knows it.
 *
 * Otherwise it falls back to the per-track search across the three catalogs ({@link
 * resolveMissingTrack}), on click: a free mxb-mods track queues straight into the installer
 * (the same pipeline Browse's quick-install uses), a Hub track opens its page, and a paid shop
 * track shows its price and links out to buy. When nothing matches confidently it offers a plain
 * search rather than guessing.
 */
export default function TrackContentAction({ track, known, pending, compact, onOpenHub }: Props) {
  const t = useT();
  const { resolved } = useI18n();
  const { game } = useConfig();
  const { startPendingInstall } = useInstall();

  const [source, setSource] = useState<TrackSource | null>(null);
  const [searching, setSearching] = useState(false);
  const [queued, setQueued] = useState(false);

  const find = useCallback(async () => {
    setSearching(true);
    try {
      setSource(await resolveMissingTrack(track, resolved));
    } catch {
      setSource({ kind: "none" });
    } finally {
      setSearching(false);
    }
  }, [track, resolved]);

  const queueMxbMods = useCallback(
    (slug: string, title: string) => {
      const trackType = modTypesFor(game.id).find((mt) => mt.id === "tracks");
      if (!trackType) return;
      startPendingInstall({
        slug,
        title,
        subpath: trackType.installSubpath,
        resolve: async () => {
          const r = await resolveQuickInstall(slug, trackType, game, trackType.categoryId);
          return r.ok ? r.params : null;
        },
      });
      setQueued(true);
      toast.success(t("serverBrowser.dl.queued", { title }));
    },
    [game, startPendingInstall, t],
  );

  const base =
    "inline-flex items-center gap-1.5 rounded-md font-medium transition-colors cursor-default " +
    (compact ? "px-2 py-1 text-[11px]" : "px-2.5 py-1.5 text-[12px]");

  if (queued) {
    return (
      <span className={cn(base, "text-success")}>
        <Download className="size-3.5" /> {t("serverBrowser.dl.queuedShort")}
      </span>
    );
  }

  // The index already found it: install, with the post's own title on the tooltip. Skipped
  // once the user has run a search of their own, so their result is never overwritten.
  if (source === null && known) {
    return (
      <button
        onClick={() => queueMxbMods(known.slug, known.title)}
        className={cn(base, "bg-primary text-primary-foreground hover:brightness-105")}
        title={t("serverBrowser.dl.installTitle", { title: known.title })}
      >
        <Download className="size-3.5" /> {t("serverBrowser.dl.install")}
      </button>
    );
  }

  // The index is still reading pages for this one. Saying "search" here would push the user
  // into work that may be about to be done for them.
  if (source === null && pending) {
    return (
      <span className={cn(base, "text-muted-foreground")} title={t("serverBrowser.dl.lookingTitle")}>
        <Loader2 className="size-3.5 animate-spin" /> {t("serverBrowser.dl.looking")}
      </span>
    );
  }

  // Before searching: a single "find it" affordance.
  if (source === null) {
    return (
      <button
        onClick={find}
        disabled={searching}
        className={cn(base, "border border-white/[0.12] text-foreground/80 hover:bg-white/[0.05] disabled:opacity-60")}
        title={t("serverBrowser.dl.findTitle")}
      >
        {searching ? <Loader2 className="size-3.5 animate-spin" /> : <Download className="size-3.5" />}
        {t(searching ? "serverBrowser.dl.searching" : "serverBrowser.dl.find")}
      </button>
    );
  }

  if (source.kind === "mxbMods") {
    return (
      <button
        onClick={() => queueMxbMods(source.mod.slug, source.title)}
        className={cn(base, "bg-primary text-primary-foreground hover:brightness-105")}
        title={source.title}
      >
        <Download className="size-3.5" /> {t("serverBrowser.dl.install")}
      </button>
    );
  }

  if (source.kind === "hub") {
    return (
      <button
        onClick={() => (onOpenHub ? onOpenHub() : void openShopUrl(source.mod.url))}
        className={cn(base, "border border-white/[0.12] text-foreground/80 hover:bg-white/[0.05]")}
        title={source.title}
      >
        {source.free ? (
          <>
            <Download className="size-3.5" /> {t("serverBrowser.dl.onHub")}
          </>
        ) : (
          <>
            <Tag className="size-3.5" /> {source.price ?? t("serverBrowser.dl.onHub")}
          </>
        )}
      </button>
    );
  }

  if (source.kind === "shop") {
    return (
      <button
        onClick={() => void openShopUrl(source.mod.url)}
        className={cn(base, "border border-amber-400/30 text-amber-300 hover:bg-amber-400/10")}
        title={t("serverBrowser.dl.onShop", { title: source.title })}
      >
        <Tag className="size-3.5" /> {source.price ?? ""}
        <ExternalLink className="size-3" />
      </button>
    );
  }

  // Nothing matched confidently: offer a manual search on the catalog.
  //
  // Opened with the shell directly rather than through `openShopUrl`, which only ever opens
  // the two *stores* and drops anything else on the floor without a word. The catalog is not
  // one of them, so routing a search through it made this button do nothing at all.
  //
  // The host comes from the active title (`catalogDomain`), not a constant: GP Bikes has its
  // own catalog, and a hardcoded mxb-mods.com would search the wrong site for its players.
  return (
    <button
      onClick={() =>
        void open(
          `https://${game.catalogDomain}/?s=${encodeURIComponent(trackQuery(track))}`,
        )
      }
      className={cn(base, "text-muted-foreground hover:text-foreground")}
      title={t("serverBrowser.dl.searchTitle")}
    >
      <Search className="size-3.5" /> {t("serverBrowser.dl.search")}
    </button>
  );
}
