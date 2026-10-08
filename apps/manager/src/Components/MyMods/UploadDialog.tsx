/**
 * The upload form: a file (picked or dropped), what it is, and who can see it. Every check in
 * `lib/modUpload.ts` runs before anything is hashed or sent; the control plane checks again.
 */
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { FileArchive, ImageIcon, Loader2, Upload, X } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Label } from "@frost/shared/Components/ui/label";
import { Combobox } from "@frost/shared/Components/ui/combobox";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@frost/shared/Components/ui/select";
import { formatBytes } from "@frost/shared/lib/mods";
import { scanBikeTargets } from "@frost/shared/api/mods";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import {
  inspectUploadFile,
  MOD_KINDS,
  startUpload,
  type ModKind,
  type MyMod,
  type PickedFile,
  type UploadJob,
} from "../../api/modUpload";
import {
  EMPTY_FORM,
  fieldProblems,
  fileProblem,
  MAX_DESCRIPTION,
  needsBike,
  THUMB_EXTENSIONS,
  toMeta,
  type FieldProblem,
  type FileProblem,
  type UploadForm,
} from "../../lib/modUpload";
import { claimDrops } from "../Dropzone/dropClaim";

export const KIND_LABEL: Record<ModKind, TKey> = {
  paints: "modKind.paints",
  bikes: "modKind.bikes",
  liveries: "modKind.liveries",
  kits: "modKind.kits",
  tracks: "modKind.tracks",
  other: "modKind.other",
};

const FIELD_LABEL: Record<FieldProblem["code"], TKey> = {
  type: "uploadErr.type",
  pntType: "uploadErr.pntType",
  title: "uploadErr.title",
  titleLong: "uploadErr.titleLong",
  bike: "uploadErr.bike",
  bikeLong: "uploadErr.bikeLong",
  descriptionLong: "uploadErr.descriptionLong",
  versionLong: "uploadErr.versionLong",
  notesLong: "uploadErr.notesLong",
};

export function useFileProblemText() {
  const t = useT();
  return (p: FileProblem) =>
    p.code === "fileTooBig"
      ? t("uploadErr.fileTooBig", { max: formatBytes(p.max) })
      : t(p.code === "noFile" ? "uploadErr.noFile" : p.code === "fileType" ? "uploadErr.fileType" : "uploadErr.fileEmpty");
}

const NEW_MOD = "new";

export default function UploadDialog({
  open,
  onOpenChange,
  mods,
  versionOf,
  limits,
  onStarted,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The rider's own mods, for "new version of". */
  mods: MyMod[];
  /** Opened from a mod's "New version". */
  versionOf: MyMod | null;
  limits: string;
  onStarted: (job: UploadJob) => void;
}) {
  const t = useT();
  const fileText = useFileProblemText();
  const [file, setFile] = useState<PickedFile | null>(null);
  const [form, setForm] = useState<UploadForm>(EMPTY_FORM);
  const [bikes, setBikes] = useState<string[]>([]);
  const [over, setOver] = useState(false);
  const [tried, setTried] = useState(false);
  const [sending, setSending] = useState(false);

  // A fresh form each time it opens, seeded from the mod a "New version" came from.
  useEffect(() => {
    if (!open) return;
    setFile(null);
    setTried(false);
    setForm(
      versionOf
        ? { ...EMPTY_FORM, assetId: versionOf.id, title: versionOf.title, type: versionOf.modType as ModKind, visibility: versionOf.visibility }
        : EMPTY_FORM,
    );
  }, [open, versionOf]);

  useEffect(() => {
    if (open && bikes.length === 0) void scanBikeTargets().then(setBikes).catch(() => {});
  }, [open, bikes.length]);

  const choose = async (path: string) => {
    try {
      setFile(await inspectUploadFile(path));
    } catch (e) {
      toast.error(t("uploadErr.fileType"), { description: String(e) });
    }
  };

  // While open, a drop anywhere in the window is this file.
  useEffect(() => {
    if (!open) return;
    return claimDrops({
      onOver: setOver,
      onDrop: (paths) => {
        if (paths[0]) void choose(paths[0]);
      },
    });
    // `choose` only closes over `t`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const browse = async () => {
    const picked = await pickFile({
      multiple: false,
      directory: false,
      filters: [{ name: "MX Bikes mod", extensions: ["pkz", "zip", "pnt"] }],
    });
    if (typeof picked === "string") await choose(picked);
  };

  const fileIssue = fileProblem(file);
  const issues = useMemo(() => fieldProblems(form, file), [form, file]);
  const problemText = (field: FieldProblem["field"]) => {
    const p = issues.find((i) => i.field === field);
    return p ? t(FIELD_LABEL[p.code]) : undefined;
  };
  const set = <K extends keyof UploadForm>(k: K, v: UploadForm[K]) => setForm((f) => ({ ...f, [k]: v }));
  const fresh = form.assetId === null;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setTried(true);
    if (fileIssue || issues.length || !file || sending) return;
    setSending(true);
    try {
      const job = await startUpload(file.path, toMeta(form));
      toast.success(t("upload.started"));
      onStarted(job);
      onOpenChange(false);
    } catch (err) {
      toast.error(t("upload.startFailed"), { description: String(err) });
    }
    setSending(false);
  };

  const browsePicture = async () => {
    const picked = await pickFile({ multiple: false, filters: [{ name: "Image", extensions: THUMB_EXTENSIONS }] });
    if (typeof picked === "string") set("thumbPath", picked);
  };

  const pickVersionOf = (v: string) => {
    if (v === NEW_MOD) return setForm({ ...EMPTY_FORM });
    const m = mods.find((x) => x.id === v);
    if (m) setForm((f) => ({ ...f, assetId: m.id, title: m.title, type: m.modType as ModKind, visibility: m.visibility }));
  };

  const editable = mods.filter((m) => m.state === "active" || m.state === "hidden");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>
            {versionOf ? t("upload.newVersionTitle", { title: versionOf.title }) : t("upload.title")}
          </DialogTitle>
          <DialogDescription>{limits}</DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex max-h-[65vh] flex-col gap-3 overflow-y-auto pr-1">
          <button
            type="button"
            onClick={() => void browse()}
            className={cn(
              "flex cursor-default items-center gap-3 rounded-xl border border-dashed p-4 text-left transition-colors",
              over ? "border-primary bg-primary/5" : "border-white/[0.12] hover:border-white/25",
            )}
          >
            <FileArchive className="size-5 flex-none text-muted-foreground" />
            {file ? (
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] font-semibold">{file.filename}</span>
                <span className="text-[11.5px] text-muted-foreground">{formatBytes(file.size)}</span>
              </span>
            ) : (
              <span className="flex-1 text-[12.5px] text-muted-foreground">{t("upload.drop")}</span>
            )}
            <span className="text-[12px] font-semibold text-primary">
              {file ? t("upload.change") : t("upload.pick")}
            </span>
          </button>
          {file && fileIssue && <Problem text={fileText(fileIssue)} />}
          {!file && tried && <Problem text={fileText({ code: "noFile" })} />}

          {!versionOf && editable.length > 0 && (
            <Field label={t("upload.fieldVersionOf")}>
              <Select value={form.assetId === null ? NEW_MOD : form.assetId} onValueChange={pickVersionOf}>
                <SelectTrigger className="h-9">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NEW_MOD}>{t("upload.fieldNewMod")}</SelectItem>
                  {editable.map((m) => (
                    <SelectItem key={m.id} value={m.id}>
                      {m.title}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          )}

          {fresh && (
            <Field label={t("upload.fieldType")} problem={tried ? problemText("type") : undefined}>
              <Select value={form.type} onValueChange={(v) => set("type", v as ModKind)}>
                <SelectTrigger className="h-9">
                  <SelectValue placeholder={t("upload.fieldTypePick")} />
                </SelectTrigger>
                <SelectContent>
                  {MOD_KINDS.map((k) => (
                    <SelectItem key={k} value={k}>
                      {t(KIND_LABEL[k])}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          )}

          <Field label={t("upload.fieldTitle")} problem={tried ? problemText("title") : undefined}>
            <Input value={form.title} onChange={(e) => set("title", e.target.value)} maxLength={120} />
          </Field>

          {needsBike(form.type) && (
            <Field label={t("upload.fieldBike")} problem={tried ? problemText("bike") : undefined}>
              <Combobox
                value={form.bike}
                options={bikes}
                onChange={(v) => set("bike", v)}
                placeholder={t("upload.fieldBikePick")}
                allowEmpty={false}
                className="w-full"
              />
            </Field>
          )}

          <Field label={t("upload.fieldDescription")} problem={tried ? problemText("description") : undefined}>
            <textarea
              value={form.description}
              onChange={(e) => set("description", e.target.value)}
              maxLength={MAX_DESCRIPTION}
              rows={4}
              className="w-full resize-y rounded-lg border border-input bg-transparent px-3 py-2 text-[13px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            />
          </Field>

          <Field label={t("upload.fieldPicture")}>
            <div className="flex items-center gap-2">
              <Button type="button" variant="outline" size="sm" onClick={() => void browsePicture()}>
                <ImageIcon className="size-3.5" /> {t("upload.pickPicture")}
              </Button>
              {form.thumbPath && (
                <>
                  <span className="min-w-0 truncate text-[12px] text-muted-foreground">{form.thumbPath.split(/[\\/]/).pop()}</span>
                  <Button type="button" variant="ghost" size="icon" className="h-7 w-7" onClick={() => set("thumbPath", null)}>
                    <X className="size-3.5" />
                  </Button>
                </>
              )}
            </div>
          </Field>

          <div className="flex gap-3">
            <Field label={t("upload.fieldVersion")} problem={tried ? problemText("version") : undefined} className="flex-1">
              <Input value={form.version} onChange={(e) => set("version", e.target.value)} maxLength={40} placeholder="v1" />
            </Field>
            {fresh && (
              <Field label={t("upload.fieldVisibility")}>
                <Segmented
                  value={form.visibility}
                  onChange={(v) => set("visibility", v)}
                  options={[
                    { value: "public", label: t("upload.public") },
                    { value: "unlisted", label: t("upload.unlisted") },
                  ]}
                />
              </Field>
            )}
          </div>
          {fresh && (
            <p className="-mt-1 text-[11.5px] text-faint">
              {t(form.visibility === "public" ? "upload.publicHint" : "upload.unlistedHint")}
            </p>
          )}

          {!fresh && (
            <Field label={t("upload.fieldNotes")} problem={tried ? problemText("notes") : undefined}>
              <Input value={form.notes} onChange={(e) => set("notes", e.target.value)} maxLength={2000} />
            </Field>
          )}

          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={sending}>
              {sending ? <Loader2 className="size-3.5 animate-spin" /> : <Upload className="size-3.5" />}
              {t("upload.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  problem,
  className,
  children,
}: {
  label: string;
  problem?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label>{label}</Label>
      {children}
      {problem && <Problem text={problem} />}
    </div>
  );
}
function Problem({ text }: { text: string }) {
  return <p className="text-[11.5px] text-destructive">{text}</p>;
}
