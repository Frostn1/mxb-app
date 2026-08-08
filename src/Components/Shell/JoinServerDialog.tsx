import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Loader2, Plug } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";
import { Input } from "@/Components/ui/input";
import { Button } from "@/Components/ui/button";
import { cpServers, joinServer, type RegisteredServer } from "../../api/mods";
import { useT } from "../../i18n/context";

/** Remembers the last address, so rejoining a regular server is one keystroke. */
const LAST_ADDRESS_KEY = "mxb:lastServerAddress";

interface JoinServerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Lets the sidebar refresh its running-state probe once the game is up. */
  onJoined?: () => void;
}

/**
 * Join a server by address: the app starts MX Bikes with the connect flag so the game
 * lands in the server directly, instead of the player hunting for it in the in-game list.
 *
 * The servers we know about are offered as a list, because "where do I get the address"
 * had no answer in the UI — the control plane has held a registry of them all along and
 * nothing ever showed it. Typing an address still works, for a server that isn't listed,
 * but it is the fallback rather than the only path.
 *
 * The address is validated in the Rust command rather than here — it ends up on the
 * game's command line, so the check has to sit next to the spawn, not in a UI that a
 * future caller might bypass. Its messages are what this shows on failure.
 */
const JoinServerDialog = ({
  open,
  onOpenChange,
  onJoined,
}: JoinServerDialogProps) => {
  const t = useT();
  const [address, setAddress] = useState(
    () => localStorage.getItem(LAST_ADDRESS_KEY) ?? "",
  );
  const [joining, setJoining] = useState(false);
  // `null` while loading. Empty means either nothing is registered or this install hasn't
  // enrolled — both land on the address field, which is the useful thing to show.
  const [servers, setServers] = useState<RegisteredServer[] | null>(null);
  const [manual, setManual] = useState(false);

  // Re-read each time it opens rather than once per mount: the registry is the one thing
  // here that changes without the player doing anything.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setServers(null);
    cpServers()
      .then((list) => !cancelled && setServers(list))
      .catch(() => !cancelled && setServers([]));
    return () => {
      cancelled = true;
    };
  }, [open]);

  const join = async (target: string) => {
    if (joining || !target.trim()) return;
    setJoining(true);
    try {
      const outcome = await joinServer(target);
      if (outcome === "already_running") {
        toast.info(t("join.alreadyRunning"));
      } else {
        localStorage.setItem(LAST_ADDRESS_KEY, target.trim());
        toast.success(t("join.launching", { address: target.trim() }));
        onOpenChange(false);
        onJoined?.();
      }
    } catch (e) {
      toast.error(t("join.failed"), { description: String(e) });
    }
    setJoining(false);
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void join(address);
  };

  const showList = servers === null || servers.length > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("join.title")}</DialogTitle>
          <DialogDescription>{t("join.desc")}</DialogDescription>
        </DialogHeader>

        {showList && (
          <div className="mt-4 space-y-1.5">
            {servers === null ? (
              <div className="h-14 animate-pulse rounded-lg bg-white/[0.04]" />
            ) : (
              servers.map((s) => (
                <button
                  key={s.id}
                  disabled={joining}
                  onClick={() => void join(s.address)}
                  className="flex w-full cursor-default items-center gap-3 rounded-lg border border-white/[0.07] px-3 py-2.5 text-left transition-colors hover:bg-white/[0.04] disabled:opacity-50"
                >
                  <Plug className="size-4 flex-none text-muted-foreground" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] font-medium">{s.name}</span>
                    <span className="block truncate text-[11.5px] text-muted-foreground">
                      {s.address}
                    </span>
                  </span>
                  <span className="flex-none text-[11px] uppercase text-muted-foreground">
                    {s.region}
                  </span>
                </button>
              ))
            )}
          </div>
        )}

        {/* The address field is the escape hatch for a server we don't list. It's hidden
            when there is a list to pick from, so the common case is one click. */}
        {servers !== null && (servers.length === 0 || manual) ? (
          <form onSubmit={onSubmit}>
            <label className="mt-4 block text-[12.5px] text-muted-foreground">
              {t("join.address")}
              <Input
                autoFocus
                value={address}
                spellCheck={false}
                onChange={(e) => setAddress(e.target.value)}
                placeholder="203.0.113.10:54210"
                className="mt-1.5"
              />
            </label>
            {servers.length === 0 && (
              <p className="mt-2 text-[11.5px] text-muted-foreground">{t("join.noServers")}</p>
            )}
            <DialogFooter className="mt-5">
              <Button type="submit" disabled={joining || !address.trim()}>
                {joining && <Loader2 className="size-4 animate-spin" />}
                {joining ? t("join.joining") : t("join.action")}
              </Button>
            </DialogFooter>
          </form>
        ) : (
          servers !== null && (
            <button
              onClick={() => setManual(true)}
              className="mt-3 cursor-default text-left text-[11.5px] text-muted-foreground underline underline-offset-2 hover:text-foreground"
            >
              {t("join.manual")}
            </button>
          )
        )}
      </DialogContent>
    </Dialog>
  );
};

export default JoinServerDialog;
