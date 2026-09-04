/**
 * Feel presets — the settings half of a profile, saved and switched by name.
 *
 * Separate from look presets on purpose: a rider keeps one look and two feels, or the other
 * way round, and pairing them would force a duplicate every time either half changed.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Loader2,
  Save,
  Play,
  Share2,
  Download,
  Trash2,
  Copy,
  Check,
  Gauge,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { Button, CHIP } from "../ui/button";
import { Input } from "../ui/input";
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from "../ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
} from "../ui/dialog";
import {
  feelList,
  feelCapture,
  feelSave,
  feelDelete,
  feelApply,
  feelExport,
  feelImport,
  type Feel,
} from "../../api/mods";
import { useT } from "../../i18n/context";
import { copyText } from "../../lib/clipboard";

/** How many settings and how many controls a preset carries. */
function summarise(feel: Feel) {
  const settings = Object.values(feel.ini ?? {}).reduce(
    (n, keys) => n + Object.keys(keys).length,
    0,
  );
  return { settings, controls: Object.keys(feel.controls ?? {}).length };
}

interface Props {
  profiles: string[];
  profile: string;
  onProfile: (profile: string) => void;
}

export default function FeelPresets({ profiles, profile, onProfile }: Props) {
  const t = useT();
  const [saved, setSaved] = useState<Feel[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [applying, setApplying] = useState<string | null>(null);
  const [share, setShare] = useState<{ name: string; code: string } | null>(null);
  const [importOpen, setImportOpen] = useState(false);

  const load = useCallback(async () => {
    setSaved(await feelList());
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const saveCurrent = useCallback(async () => {
    const nm = name.trim();
    if (!nm) {
      toast.error(t("feel.nameFirst"));
      return;
    }
    if (!profile) {
      toast.error(t("feel.pickProfile"));
      return;
    }
    setBusy(true);
    try {
      const feel = await feelCapture(profile);
      const { settings, controls } = summarise(feel);
      if (settings === 0 && controls === 0) {
        toast.error(t("feel.nothingToSave"));
        return;
      }
      await feelSave({ ...feel, name: nm });
      setName("");
      await load();
      toast.success(t("feel.saved", { name: nm }), {
        description: t("feel.summary", { settings, controls }),
      });
    } catch (e) {
      toast.error(t("feel.saveFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }, [name, profile, load, t]);

  const apply = useCallback(
    async (feel: Feel) => {
      if (!profile) {
        toast.error(t("feel.pickProfile"));
        return;
      }
      setApplying(feel.name);
      try {
        const report = await feelApply(profile, feel.name);
        const missing = report.missingControls;
        toast.success(t("feel.applied", { name: feel.name }), {
          description: missing.length
            ? t("feel.missingControls", { names: missing.join(", ") })
            : t("feel.restartHint"),
        });
      } catch (e) {
        toast.error(t("feel.applyFailed"), { description: String(e) });
      } finally {
        setApplying(null);
      }
    },
    [profile, t],
  );

  const remove = useCallback(
    async (feel: Feel) => {
      await feelDelete(feel.name);
      await load();
      toast.success(t("feel.deleted", { name: feel.name }));
    },
    [load, t],
  );

  const openShare = useCallback(
    async (feel: Feel) => {
      try {
        setShare({ name: feel.name, code: await feelExport(feel.name) });
      } catch (e) {
        toast.error(t("feel.shareFailed"), { description: String(e) });
      }
    },
    [t],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-7 pb-6">
      {/* Capture row */}
      <div className="flex flex-wrap items-end gap-3 rounded-xl border border-white/[0.07] bg-card/40 p-3.5">
        <label className="flex min-w-[140px] flex-col gap-1">
          <span className="text-[11px] font-medium text-muted-foreground">
            {t("feel.profile")}
          </span>
          <Select value={profile} onValueChange={onProfile}>
            <SelectTrigger className="h-8 w-[180px]">
              <SelectValue placeholder={t("feel.profile")} />
            </SelectTrigger>
            <SelectContent>
              {profiles.map((p) => (
                <SelectItem key={p} value={p}>
                  {p}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
        <label className="flex min-w-[180px] flex-1 flex-col gap-1">
          <span className="text-[11px] font-medium text-muted-foreground">
            {t("feel.saveCurrent")}
          </span>
          <Input
            className="h-8"
            value={name}
            placeholder={t("feel.namePlaceholder")}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void saveCurrent();
            }}
          />
        </label>
        <Button size="sm" onClick={() => void saveCurrent()} disabled={busy}>
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
          {t("feel.save")}
        </Button>
        <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
          <Download className="size-3.5" />
          {t("feel.import")}
        </Button>
      </div>

      <p className="-mt-1 px-1 text-[12px] leading-relaxed text-muted-foreground">
        {t("feel.saveHint")}
      </p>

      {/* Saved list */}
      <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
        {t("feel.savedTitle")}
      </h2>
      {saved.length === 0 ? (
        <div className="rounded-xl border border-white/[0.07] bg-card/40 px-4 py-6 text-center text-[12.5px] text-muted-foreground">
          {t("feel.noneHint")}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          {saved.map((feel) => {
            const { settings, controls } = summarise(feel);
            return (
              <div
                key={feel.name}
                className="flex flex-col gap-2.5 rounded-xl border border-white/[0.07] bg-card/40 p-3.5"
              >
                <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5">
                    <Gauge className="size-3.5 flex-none text-muted-foreground" />
                    <span className="truncate text-[13px] font-semibold">{feel.name}</span>
                  </div>
                  <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
                    {t("feel.summary", { settings, controls })}
                  </div>
                </div>
                <div className="flex flex-none items-center gap-0.5">
                  <IconBtn chip title={t("feel.share")} onClick={() => void openShare(feel)}>
                    <Share2 className="size-3.5" />
                  </IconBtn>
                  <IconBtn title={t("common.delete")} onClick={() => void remove(feel)}>
                    <Trash2 className="size-3.5" />
                  </IconBtn>
                </div>
                </div>
                <Button
                  size="sm"
                  className="h-7 w-full"
                  disabled={applying !== null}
                  onClick={() => void apply(feel)}
                >
                  {applying === feel.name ? (
                    <Loader2 className="size-3.5 animate-spin" />
                  ) : (
                    <Play className="size-3.5" />
                  )}
                  {t("feel.apply")}
                </Button>
              </div>
            );
          })}
        </div>
      )}

      <ShareDialog share={share} onClose={() => setShare(null)} />
      <ImportDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onDone={() => void load()}
      />
    </div>
  );
}

function ShareDialog({
  share,
  onClose,
}: {
  share: { name: string; code: string } | null;
  onClose: () => void;
}) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  useEffect(() => setCopied(false), [share]);

  return (
    <Dialog open={share !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("feel.shareTitle", { name: share?.name ?? "" })}</DialogTitle>
          <DialogDescription>{t("feel.shareBody")}</DialogDescription>
        </DialogHeader>
        <div className="max-h-[160px] overflow-y-auto break-all rounded-lg border border-border bg-card/40 px-3 py-2 font-mono text-[11px] text-foreground/80">
          {share?.code}
        </div>
        <DialogFooter>
          <Button
            size="sm"
            onClick={async () => {
              if (!share) return;
              await copyText(share.code);
              setCopied(true);
            }}
          >
            {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
            {copied ? t("presets.copiedShare") : t("presets.copyCode")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ImportDialog({
  open,
  onClose,
  onDone,
}: {
  open: boolean;
  onClose: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) setText("");
  }, [open]);

  const run = useCallback(async () => {
    setBusy(true);
    try {
      const feel = await feelImport(text);
      onDone();
      onClose();
      toast.success(t("feel.imported", { name: feel.name }));
    } catch (e) {
      toast.error(t("feel.importFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }, [text, onDone, onClose, t]);

  const canRun = useMemo(() => text.trim().length > 0 && !busy, [text, busy]);

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("feel.importTitle")}</DialogTitle>
          <DialogDescription>{t("feel.importBody")}</DialogDescription>
        </DialogHeader>
        <Input
          value={text}
          placeholder={t("feel.importPlaceholder")}
          onChange={(e) => setText(e.target.value)}
        />
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={!canRun} onClick={() => void run()}>
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Download className="size-3.5" />}
            {t("feel.import")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Same quiet icon button the look-preset card uses. */
function IconBtn({
  title,
  onClick,
  children,
  chip = false,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
  chip?: boolean;
}) {
  return (
    <button
      title={title}
      aria-label={title}
      onClick={onClick}
      className={cn(
        "cursor-default rounded-md p-1.5 transition-colors",
        chip ? CHIP : "text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}
