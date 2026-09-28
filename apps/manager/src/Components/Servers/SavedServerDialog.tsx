import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Bookmark, Loader2 } from "lucide-react";
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
import { SAVED_SERVER_ERRORS, type SavedServer } from "@frost/shared/api/mods";
import { useT, type TFunc, type TKey } from "@/i18n";

/** A saved-server command's refusal, in words. The parser's own messages pass through. */
export function savedServerError(t: TFunc<TKey>, e: unknown): string {
  if (e === SAVED_SERVER_ERRORS.duplicate) return t("savedServers.duplicate");
  if (e === SAVED_SERVER_ERRORS.full) return t("savedServers.full");
  if (e === SAVED_SERVER_ERRORS.missing) return t("savedServers.missing");
  return typeof e === "string" ? e : String(e);
}

/**
 * Save a server by `ip:port`, or change one already saved.
 *
 * The same shape as the register dialog beside it, and for the same reason: validation and
 * normalisation live in the Rust command next to Join's, so what is typed here and what Join
 * takes can't disagree. A refusal is shown under the field rather than as a toast, since it is
 * about what's in the field.
 */
const SavedServerDialog = ({
  open,
  onOpenChange,
  editing,
  onSubmit,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The server being changed, or `null` to save a new one. */
  editing: SavedServer | null;
  onSubmit: (address: string, name: string) => Promise<unknown>;
}) => {
  const t = useT();
  const [address, setAddress] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Each opening starts from the server being edited, or blank.
  useEffect(() => {
    if (!open) return;
    setAddress(editing?.address ?? "");
    setName(editing?.name ?? "");
    setError(null);
  }, [open, editing]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (saving || !address.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await onSubmit(address, name);
      if (!editing) toast.success(t("savedServers.added", { name: name.trim() || address.trim() }));
      onOpenChange(false);
    } catch (err) {
      setError(savedServerError(t, err));
    }
    setSaving(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[440px]">
        <DialogHeader>
          <DialogTitle>
            {editing ? t("savedServers.editTitle") : t("savedServers.addTitle")}
          </DialogTitle>
          <DialogDescription>{t("savedServers.blurb")}</DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-3">
          <label className="flex flex-col gap-1.5 text-[12px] text-muted-foreground">
            {t("savedServers.address")}
            <Input
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="203.0.113.10:54210"
              autoFocus
              spellCheck={false}
              aria-invalid={!!error}
            />
          </label>
          <label className="flex flex-col gap-1.5 text-[12px] text-muted-foreground">
            {t("savedServers.name")}
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("savedServers.namePlaceholder")}
              maxLength={64}
            />
          </label>
          {error && <p className="text-[12px] text-destructive">{error}</p>}
          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={saving || !address.trim()}>
              {saving ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <Bookmark className="size-3.5" />
              )}
              {t("savedServers.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default SavedServerDialog;
