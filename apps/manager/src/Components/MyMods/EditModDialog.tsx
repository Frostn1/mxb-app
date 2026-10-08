/**
 * Edit a mod's title, description, bike and visibility (`uploads.ts:395` `editMod`). Only the
 * fields the rider changed are sent, so a mod whose page can't be read yet (no live version)
 * still edits safely.
 */
import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Loader2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Label } from "@frost/shared/Components/ui/label";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { useT } from "@/i18n";
import { editMod, modDetails, type ModEdit, type MyMod, type Visibility } from "../../api/modUpload";
import { MAX_DESCRIPTION, MAX_TITLE } from "../../lib/modUpload";

interface Fields {
  title: string;
  description: string;
  bike: string;
  visibility: Visibility;
}

export default function EditModDialog({
  mod,
  onClose,
  onSaved,
}: {
  mod: MyMod | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const t = useT();
  const [start, setStart] = useState<Fields | null>(null);
  const [fields, setFields] = useState<Fields | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!mod) return;
    const base: Fields = { title: mod.title, description: "", bike: "", visibility: mod.visibility };
    setStart(base);
    setFields(base);
    let gone = false;
    void modDetails(mod.id)
      .then((d) => {
        if (gone) return;
        const full = { ...base, description: d.description, bike: d.bike.join("; ") };
        setStart(full);
        setFields(full);
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
  }, [mod]);

  const changes = (): ModEdit => {
    if (!start || !fields) return {};
    const out: ModEdit = {};
    if (fields.title.trim() !== start.title) out.title = fields.title.trim();
    if (fields.description !== start.description) out.description = fields.description;
    if (fields.bike !== start.bike) out.bike = fields.bike.trim();
    if (fields.visibility !== start.visibility) out.visibility = fields.visibility;
    return out;
  };

  const titleBad = !fields || !fields.title.trim();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const edit = changes();
    if (!mod || titleBad || saving) return;
    if (Object.keys(edit).length === 0) return onClose();
    setSaving(true);
    try {
      await editMod(mod.id, edit);
      toast.success(t("myMods.saved"));
      onSaved();
      onClose();
    } catch (err) {
      toast.error(t("myMods.saveFailed"), { description: String(err) });
    }
    setSaving(false);
  };

  const set = <K extends keyof Fields>(k: K, v: Fields[K]) => setFields((f) => (f ? { ...f, [k]: v } : f));

  return (
    <Dialog open={!!mod} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>{t("myMods.editTitle")}</DialogTitle>
        </DialogHeader>
        {fields && (
          <form onSubmit={submit} className="flex flex-col gap-3">
            <div className="flex flex-col gap-1.5">
              <Label>{t("upload.fieldTitle")}</Label>
              <Input value={fields.title} onChange={(e) => set("title", e.target.value)} maxLength={MAX_TITLE} />
              {titleBad && <p className="text-[11.5px] text-destructive">{t("uploadErr.title")}</p>}
            </div>
            <div className="flex flex-col gap-1.5">
              <Label>{t("upload.fieldDescription")}</Label>
              <textarea
                value={fields.description}
                onChange={(e) => set("description", e.target.value)}
                maxLength={MAX_DESCRIPTION}
                rows={4}
                className="w-full resize-y rounded-lg border border-input bg-transparent px-3 py-2 text-[13px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label>{t("upload.fieldBike")}</Label>
              <Input value={fields.bike} onChange={(e) => set("bike", e.target.value)} maxLength={120} />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label>{t("upload.fieldVisibility")}</Label>
              <Segmented
                value={fields.visibility}
                onChange={(v) => set("visibility", v)}
                options={[
                  { value: "public", label: t("upload.public") },
                  { value: "unlisted", label: t("upload.unlisted") },
                ]}
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" size="sm" disabled={saving || titleBad}>
                {saving && <Loader2 className="size-3.5 animate-spin" />}
                {t("myMods.save")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
