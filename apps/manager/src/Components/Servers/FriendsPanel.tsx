import { useEffect, useState } from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Loader2,
  UserPlus,
  Users,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import {
  friendsHidePresence,
  friendsRemove,
  friendsRequestAccount,
  friendsRequestCode,
  friendsRespond,
  friendsSearch,
  setFriendsPresence,
  type Friend,
  type FriendsState,
  type SearchResult,
} from "../../api/friends";
import { looksLikeCode, serverForFriend, sortFriends, type ServerRef } from "../../lib/friends";
import type { JoinAction } from "./joinAction";

const OPEN_KEY = "mxb:friendsOpen:v1";

type ActionKind = JoinAction["kind"];

function errorText(e: unknown): string {
  return typeof e === "string" ? e : String(e);
}

/**
 * Friends, above the server list.
 *
 * Who is riding right now and where, with a Join that goes through the same decision a server
 * tile makes: a friend on a track the player lacks gets "Install & join", not a button that
 * launches the game into a server it cannot load. Collapsible and remembered, like the saved
 * servers beside it.
 */
const FriendsPanel = ({
  state,
  servers,
  actionFor,
  onJoin,
  onChanged,
}: {
  state: FriendsState;
  /** The listed servers, which is where a friend's server name is looked up. */
  servers: readonly (ServerRef & { track?: string })[];
  /** What the main button would do for this server, so the label can say it. */
  actionFor: (server: ServerRef) => ActionKind;
  onJoin: (server: ServerRef) => void;
  /** Called after anything that changes the list, so the tab asks again. */
  onChanged: () => void;
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

  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const friends = sortFriends(state.friends);
  const online = friends.filter((f) => f.presence).length;

  /** Run one change, tell the player if it failed, and refresh either way. */
  const act = async (key: string, run: () => Promise<void>) => {
    setBusy(key);
    try {
      await run();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(null);
      onChanged();
    }
  };

  const [sharing, setSharing] = useState(!state.hidePresence);
  useEffect(() => setSharing(!state.hidePresence), [state.hidePresence]);
  const toggleSharing = (on: boolean) => {
    setSharing(on);
    void act("share", async () => {
      // Both halves: the app stops reporting, and the control plane stops storing.
      await setFriendsPresence(on);
      await friendsHidePresence(!on);
    });
  };

  return (
    <section className="shrink-0 px-7 pb-4">
      <div className="mb-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          title={t("friends.toggle")}
          className="flex cursor-default items-center gap-1.5 font-cond text-[11px] font-bold uppercase tracking-[0.14em] text-faint transition-colors hover:text-muted-foreground"
        >
          {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
          <Users className="size-3.5" />
          {t("friends.title")}
          <span className="tabular-nums">{friends.length}</span>
          {online > 0 && (
            <span className="tabular-nums text-success">{t("friends.onlineCount", { count: online })}</span>
          )}
        </button>
        <button
          type="button"
          onClick={() => {
            setOpen(true);
            setAdding((v) => !v);
          }}
          title={t("friends.add")}
          aria-label={t("friends.add")}
          className="grid size-5 cursor-default place-items-center rounded-md text-faint transition-colors hover:text-foreground"
        >
          <UserPlus className="size-3.5" />
        </button>
      </div>

      {open && (
        <div className="flex flex-col gap-3">
          {state.incoming.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <span className="font-cond text-[10.5px] font-bold uppercase tracking-[0.14em] text-faint">
                {t("friends.requests")}
              </span>
              {state.incoming.map((p) => (
                <div
                  key={p.accountId}
                  className="flex items-center gap-2 rounded-xl border border-input bg-card px-3 py-2 text-[13px]"
                >
                  <span className="min-w-0 flex-1 truncate font-semibold">{p.riderName}</span>
                  <Button
                    size="sm"
                    disabled={busy !== null}
                    onClick={() =>
                      void act(p.accountId, async () => {
                        await friendsRespond(p.accountId, true);
                        toast.success(t("friends.nowFriends", { name: p.riderName }));
                      })
                    }
                  >
                    <Check className="size-3.5" />
                    {t("friends.accept")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy !== null}
                    onClick={() => void act(p.accountId, async () => void (await friendsRespond(p.accountId, false)))}
                  >
                    <X className="size-3.5" />
                    {t("friends.decline")}
                  </Button>
                </div>
              ))}
            </div>
          )}

          {adding && (
            <AddFriend
              onDone={() => {
                setAdding(false);
                onChanged();
              }}
            />
          )}

          {friends.length === 0 ? (
            <p className="text-[12.5px] text-faint">{t("friends.empty")}</p>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
              {friends.map((f) => (
                <FriendCard
                  key={f.accountId}
                  friend={f}
                  server={serverForFriend(f, servers)}
                  action={(s) => actionFor(s)}
                  busy={busy !== null}
                  onJoin={onJoin}
                  onRemove={() =>
                    void act(f.accountId, async () => {
                      await friendsRemove(f.accountId);
                      toast.success(t("friends.removed", { name: f.riderName }));
                    })
                  }
                />
              ))}
            </div>
          )}

          {state.outgoing.length > 0 && (
            <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-faint">
              <span className="font-cond text-[10.5px] font-bold uppercase tracking-[0.14em]">
                {t("friends.pending")}
              </span>
              {state.outgoing.map((p) => (
                <span key={p.accountId} className="inline-flex items-center gap-1">
                  {p.riderName}
                  <button
                    type="button"
                    title={t("friends.cancel")}
                    aria-label={t("friends.cancel")}
                    disabled={busy !== null}
                    onClick={() => void act(p.accountId, async () => void (await friendsRemove(p.accountId)))}
                    className="grid size-4 cursor-default place-items-center rounded text-faint hover:text-foreground"
                  >
                    <X className="size-3" />
                  </button>
                </span>
              ))}
            </p>
          )}

          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-input pt-3 text-[12px] text-muted-foreground">
            <span className="inline-flex items-center gap-2">
              {t("friends.yourCode")}
              <code className="rounded-md bg-card px-2 py-0.5 font-mono text-[12.5px] tracking-wider text-foreground">
                {state.friendCode}
              </code>
              <button
                type="button"
                title={t("friends.copyCode")}
                aria-label={t("friends.copyCode")}
                onClick={() =>
                  navigator.clipboard
                    .writeText(state.friendCode)
                    .then(() => toast.success(t("friends.codeCopied")))
                    .catch(() => {})
                }
                className="grid size-5 cursor-default place-items-center rounded-md text-faint hover:text-foreground"
              >
                <Copy className="size-3.5" />
              </button>
            </span>
            <label className="inline-flex items-center gap-2" title={t("friends.sharePresenceDesc")}>
              <Switch checked={sharing} onCheckedChange={toggleSharing} disabled={busy === "share"} />
              {t("friends.sharePresence")}
            </label>
          </div>
        </div>
      )}
    </section>
  );
};

/** One friend: who, where, and the button that goes there. */
const FriendCard = ({
  friend,
  server,
  action,
  busy,
  onJoin,
  onRemove,
}: {
  friend: Friend;
  server: ServerRef | null;
  action: (server: ServerRef) => ActionKind;
  busy: boolean;
  onJoin: (server: ServerRef) => void;
  onRemove: () => void;
}) => {
  const t = useT();
  const p = friend.presence;
  const kind = server ? action(server) : null;
  const label =
    kind === "install"
      ? t("friends.installJoin")
      : kind === "wait"
        ? t("friends.wait")
        : kind === "join" || kind === "activate" || kind === "failed"
          ? t("friends.join")
          : t("friends.view");

  return (
    <div
      className={cn(
        "group flex items-center gap-3 rounded-xl border border-white/[0.07] bg-card px-3 py-2.5",
        !p && "opacity-60",
      )}
    >
      <span
        className={cn(
          "relative grid size-8 shrink-0 place-items-center rounded-full bg-foreground/10 text-[13px] font-bold uppercase",
        )}
        aria-hidden
      >
        {friend.riderName.slice(0, 1)}
        <span
          className={cn(
            "absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full border-2 border-card",
            p ? "bg-success" : "bg-foreground/30",
          )}
        />
      </span>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-semibold" title={friend.riderName}>
          {friend.riderName}
        </div>
        <div className="truncate text-[11.5px] text-muted-foreground" title={p?.serverName}>
          {p ? t("friends.on", { server: p.serverName }) : t("friends.offline")}
          {p?.riders != null && p.riders > 0 && <span className="text-faint"> · {p.riders}</span>}
        </div>
      </div>
      {p &&
        (server ? (
          <Button size="sm" disabled={busy} onClick={() => onJoin(server)}>
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : null}
            {label}
          </Button>
        ) : (
          <span className="shrink-0 text-[11px] text-faint" title={t("friends.notListed")}>
            {t("friends.notListedShort")}
          </span>
        ))}
      <button
        type="button"
        onClick={onRemove}
        title={t("friends.remove")}
        aria-label={t("friends.remove")}
        className="grid size-5 shrink-0 cursor-default place-items-center rounded-md text-faint opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100 focus-visible:opacity-100"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
};

/** Search by name, or paste a friend code. */
const AddFriend = ({ onDone }: { onDone: () => void }) => {
  const t = useT();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [working, setWorking] = useState(false);
  const code = looksLikeCode(query);

  // Searched as the player types, after a pause, once there is enough to search for.
  useEffect(() => {
    const q = query.trim();
    if (code || q.length < 3) {
      setResults(null);
      return;
    }
    let live = true;
    const id = window.setTimeout(() => {
      friendsSearch(q)
        .then((r) => live && setResults(r))
        .catch((e: unknown) => live && (toast.error(errorText(e)), setResults([])));
    }, 300);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [query, code]);

  const report = (r: { status: string; riderName: string }) =>
    toast.success(
      r.status === "friends"
        ? t("friends.nowFriends", { name: r.riderName })
        : t("friends.requestSent", { name: r.riderName }),
    );

  const sendCode = async () => {
    setWorking(true);
    try {
      report(await friendsRequestCode(query));
      onDone();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setWorking(false);
    }
  };

  const sendTo = async (p: SearchResult) => {
    setWorking(true);
    try {
      report(await friendsRequestAccount(p.accountId));
      setResults((cur) =>
        cur ? cur.map((r) => (r.accountId === p.accountId ? { ...r, relation: "pending_out" } : r)) : cur,
      );
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setWorking(false);
    }
  };

  return (
    <div className="flex flex-col gap-2 rounded-xl border border-input bg-card/50 p-3">
      <div className="flex items-center gap-2">
        <Input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && code && !working && void sendCode()}
          placeholder={t("friends.searchPlaceholder")}
          maxLength={32}
          className="h-8 max-w-[320px]"
        />
        {code && (
          <Button size="sm" disabled={working} onClick={() => void sendCode()}>
            {t("friends.send")}
          </Button>
        )}
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t("friends.close")}
        </Button>
      </div>
      {results === null ? (
        <p className="text-[12px] text-faint">{t("friends.searchHint")}</p>
      ) : results.length === 0 ? (
        <p className="text-[12px] text-faint">{t("friends.noResults")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {results.map((r) => (
            <li key={r.accountId} className="flex items-center gap-2 text-[13px]">
              <span className="min-w-0 flex-1 truncate">{r.riderName}</span>
              {r.relation === "none" ? (
                <Button size="sm" disabled={working} onClick={() => void sendTo(r)}>
                  <UserPlus className="size-3.5" />
                  {t("friends.send")}
                </Button>
              ) : (
                <span className="text-[11.5px] text-faint">
                  {r.relation === "friends" ? t("friends.alreadyFriends") : t("friends.requested")}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};

export default FriendsPanel;
