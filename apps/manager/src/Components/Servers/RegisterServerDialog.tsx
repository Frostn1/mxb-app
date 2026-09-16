import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Loader2, ServerCog } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@frost/shared/Components/ui/dialog";
import { Input } from "@frost/shared/Components/ui/input";
import { Button } from "@frost/shared/Components/ui/button";
import { registerServerAddress } from "@frost/shared/api/mods";
import { useT } from "@/i18n";

/**
 * Put your own server on the shared address book.
 *
 * Almost nobody needs this, and that is the point of it being a small dialog behind a small
 * button rather than anything more prominent. The app already contributes every address a
 * master sweep turns up, so a server that appears in the game's own list gets onto the shared
 * book by itself, with nobody doing anything. What it cannot do is carry a server nobody has
 * found yet, or one that was never in that list to be seen in — a private league box, a server
 * that came up an hour ago. Those are the two cases this exists for.
 *
 * The reason it needs an account, when contributing does not, is the same reason contributing
 * is safe without one: the control plane holds an anonymous address back until distinct
 * networks have independently seen it, because this list is what tells thousands of apps where
 * to send a datagram. An account is what stands in for that corroboration — and unlike a
 * sighting it is recorded against somebody.
 *
 * Validation and normalisation both live in the Rust command, next to the existing Join flow's,
 * so a typed address and a joined one cannot end up as two different servers. Its messages are
 * what this shows.
 */
const RegisterServerDialog = ({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const t = useT();
  const [address, setAddress] = useState("");
  const [saving, setSaving] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (saving || !address.trim()) return;
    setSaving(true);
    try {
      // What comes back is the address as stored — normalised, with the default port filled
      // in — which is usually not quite what was typed, and is worth showing for that reason.
      const stored = await registerServerAddress(address);
      toast.success(t("registerServer.done", { address: stored }));
      setAddress("");
      onOpenChange(false);
    } catch (e) {
      toast.error(t("registerServer.failed"), { description: String(e) });
    }
    setSaving(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[440px]">
        <DialogHeader>
          <DialogTitle>{t("registerServer.title")}</DialogTitle>
          <DialogDescription>{t("registerServer.blurb")}</DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-3">
          <Input
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="203.0.113.10:54210"
            autoFocus
            spellCheck={false}
          />
          <p className="text-[12px] leading-relaxed text-faint">{t("registerServer.note")}</p>
          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={saving || !address.trim()}>
              {saving ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <ServerCog className="size-3.5" />
              )}
              {t("registerServer.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default RegisterServerDialog;
