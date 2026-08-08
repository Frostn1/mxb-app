import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Loader2 } from "lucide-react";
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
import { joinServer } from "../../api/mods";
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

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (joining || !address.trim()) return;
    setJoining(true);
    try {
      const outcome = await joinServer(address);
      if (outcome === "already_running") {
        toast.info(t("join.alreadyRunning"));
      } else {
        localStorage.setItem(LAST_ADDRESS_KEY, address.trim());
        toast.success(t("join.launching", { address: address.trim() }));
        onOpenChange(false);
        onJoined?.();
      }
    } catch (e) {
      toast.error(t("join.failed"), { description: String(e) });
    }
    setJoining(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={onSubmit}>
          <DialogHeader>
            <DialogTitle>{t("join.title")}</DialogTitle>
            <DialogDescription>{t("join.desc")}</DialogDescription>
          </DialogHeader>

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

          <DialogFooter className="mt-5">
            <Button type="submit" disabled={joining || !address.trim()}>
              {joining && <Loader2 className="size-4 animate-spin" />}
              {joining ? t("join.joining") : t("join.action")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default JoinServerDialog;
