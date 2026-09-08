import { useEffect, useState, type FormEvent } from "react";
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
import { useT } from "../../i18n/context";

interface GuidDialogProps {
  open: boolean;
  /** The GUID currently stored by hand, or `""` when it's derived from Steam. */
  current: string;
  onClose: () => void;
  onSave: (guid: string) => void | Promise<void>;
}

/**
 * Point the Ranked tab at a GUID.
 *
 * Two people need this and nobody else: someone whose copy of MX Bikes didn't come from Steam,
 * whose GUID cannot be derived from anything on this machine, and someone who wants to look at
 * a friend's season. Clearing the field goes back to the Steam-derived one.
 *
 * A pasted profile URL works as well as a bare GUID — the Rust side takes the id off the end —
 * because copying the link is what people actually do.
 */
const GuidDialog = ({ open, current, onClose, onSave }: GuidDialogProps) => {
  const t = useT();
  const [value, setValue] = useState(current);
  const [saving, setSaving] = useState(false);

  // Re-seed each time it opens: the stored value can have changed underneath.
  useEffect(() => {
    if (open) setValue(current);
  }, [open, current]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    try {
      await onSave(value.trim());
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t("ranked.guidTitle")}</DialogTitle>
            <DialogDescription>{t("ranked.guidHelp")}</DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Input
              autoFocus
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="FF011000010178A758"
              spellCheck={false}
            />
          </div>
          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={saving}>
              {t("common.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default GuidDialog;
