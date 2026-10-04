import { useEffect, useState } from "react";
import {
  Bookmark,
  ChevronDown,
  ChevronRight,
  ArrowUp,
  ArrowDown,
  MoreHorizontal,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import type { CatalogTrack, MasterServer, SavedServer } from "@frost/shared/api/mods";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@frost/shared/Components/ui/dropdown-menu";
import { useT } from "@/i18n";
import ServerCard, { type SavedStatus } from "./ServerCard";

const OPEN_KEY = "mxb:serversSavedOpen:v1";

/** One saved server as the row draws it: what the player stored, and the server it names. */
export interface SavedRow {
  saved: SavedServer;
  /** The sweep's row, the server's own direct answer, or a stand-in when it didn't answer.
   *  What the detail pane opens on, so its rider lookup matches the server's own name. */
  row: MasterServer;
  /** {@link row} with the player's name over the server's, when they gave one. The card. */
  server: MasterServer;
  status: SavedStatus;
}

/** Everything a card needs that isn't about the saved server itself — the tab's own. */
export interface SavedCardProps {
  pictureFor: (track: string) => string | undefined;
  library: Record<string, string | null>;
  catalog: Record<string, CatalogTrack>;
  installingAt: Set<string>;
  /** Tracks whose last install failed or stalled. */
  failedTracks: Set<string>;
  /** Install the track from a file the player picks, then join. */
  onPickTrack: (s: MasterServer) => void;
  favourite: (address: string) => boolean;
  paintSync: Record<string, number>;
  /** Accepted friends on each server, by address. */
  friends: Record<string, number>;
  joining: string | null;
  queue: { address: string; position: number } | null;
  onOpen: (s: MasterServer) => void;
  onJoin: (s: MasterServer) => void;
  onWait: (s: MasterServer) => void;
  onInstall: (s: MasterServer, product: CatalogTrack) => void;
  onInstallJoin: (s: MasterServer, product: CatalogTrack) => void;
  /** Track id to the `rel` of a copy Manage has parked. */
  inactive: Record<string, string>;
  onActivateJoin: (s: MasterServer, rel: string) => void;
  onCopy: (address: string) => void;
  onToggleFavourite: (address: string) => void;
}

/**
 * The player's own short list, above the master's.
 *
 * Each server is the same card the grid uses, so it joins, queues, installs and opens its detail
 * the same way — the only difference is where its numbers came from. A saved server the sweep
 * doesn't carry is asked directly, and one that doesn't answer still shows, marked offline, with
 * Join left on: "didn't answer a datagram" is not proof the game can't get in.
 *
 * Collapsible, and remembered, because it sits above the list everyone came for.
 */
const SavedServers = ({
  rows,
  cards,
  onAdd,
  onEdit,
  onRemove,
  onMove,
}: {
  rows: SavedRow[];
  cards: SavedCardProps;
  onAdd: () => void;
  onEdit: (saved: SavedServer) => void;
  onRemove: (saved: SavedServer) => void;
  onMove: (saved: SavedServer, by: number) => void;
}) => {
  const t = useT();
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem(OPEN_KEY) !== "0";
    } catch {
      return true;
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(OPEN_KEY, open ? "1" : "0");
    } catch {
      // Storage disabled; the choice still holds for this session.
    }
  }, [open]);

  if (rows.length === 0) return null;

  return (
    <section className="shrink-0 px-7 pb-4">
      <div className="mb-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className="flex cursor-default items-center gap-1.5 font-cond text-[11px] font-bold uppercase tracking-[0.14em] text-faint transition-colors hover:text-muted-foreground"
        >
          {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          <Bookmark className="size-3.5" />
          {t("savedServers.title")}
          <span className="tabular-nums">{rows.length}</span>
        </button>
        <button
          type="button"
          onClick={onAdd}
          title={t("savedServers.add")}
          aria-label={t("savedServers.add")}
          className="grid size-5 cursor-default place-items-center rounded-md text-faint transition-colors hover:text-foreground"
        >
          <Plus className="size-3.5" />
        </button>
      </div>
      {open && (
        <div className="grid max-h-[46vh] grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3 overflow-y-auto">
          {rows.map(({ saved, server: s, status }, i) => (
            <ServerCard
              key={saved.address}
              server={s}
              status={status}
              art={cards.pictureFor(s.track)}
              missing={!!s.track && cards.library[s.track] === null}
              product={cards.catalog[s.track]}
              installing={cards.installingAt.has(s.address)}
              failed={cards.failedTracks.has(s.track)}
              onPickTrack={cards.onPickTrack}
              onInstall={cards.onInstall}
              onInstallJoin={cards.onInstallJoin}
              inactive={cards.inactive[s.track]}
              onActivateJoin={cards.onActivateJoin}
              favourite={cards.favourite(s.address)}
              paintSync={cards.paintSync[s.address] ?? 0}
              friends={cards.friends[s.address] ?? 0}
              joining={cards.joining === s.address}
              busy={cards.joining !== null}
              queuePosition={cards.queue?.address === s.address ? cards.queue.position : null}
              onOpen={cards.onOpen}
              onJoin={cards.onJoin}
              onWait={cards.onWait}
              onCopy={cards.onCopy}
              onToggleFavourite={cards.onToggleFavourite}
              menu={
                <SavedMenu
                  first={i === 0}
                  last={i === rows.length - 1}
                  onEdit={() => onEdit(saved)}
                  onRemove={() => onRemove(saved)}
                  onMove={(by) => onMove(saved, by)}
                />
              }
            />
          ))}
        </div>
      )}
    </section>
  );
};

/** Move, edit, remove: the three things only a saved card can do, behind one button so the
 *  card's own row stays Join and Copy. */
const SavedMenu = ({
  first,
  last,
  onEdit,
  onRemove,
  onMove,
}: {
  first: boolean;
  last: boolean;
  onEdit: () => void;
  onRemove: () => void;
  onMove: (by: number) => void;
}) => {
  const t = useT();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          onClick={(e) => e.stopPropagation()}
          title={t("savedServers.actions")}
          aria-label={t("savedServers.actions")}
          className="grid size-[30px] shrink-0 cursor-default place-items-center rounded-md border border-white/[0.1] text-muted-foreground transition-colors hover:text-foreground"
        >
          <MoreHorizontal className="size-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
        <DropdownMenuItem disabled={first} onSelect={() => onMove(-1)}>
          <ArrowUp className="size-4" />
          {t("savedServers.moveUp")}
        </DropdownMenuItem>
        <DropdownMenuItem disabled={last} onSelect={() => onMove(1)}>
          <ArrowDown className="size-4" />
          {t("savedServers.moveDown")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onEdit}>
          <Pencil className="size-4" />
          {t("savedServers.edit")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onRemove} className="text-destructive">
          <Trash2 className="size-4" />
          {t("savedServers.remove")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export default SavedServers;
