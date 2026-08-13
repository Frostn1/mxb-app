/**
 * Sharing installed content, and taking someone else's.
 *
 * The preset share in the Presets tab hands over a *look*. This hands over the files —
 * a track, a paint, a folder of both — as a one-line code that installs them where the
 * sender had them. Same upload, same slicing past one part; see `src-tauri/src/fileshare.rs`.
 */
import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, Check, Copy, Download, Loader2, Share2, UploadCloud } from "lucide-react";
import { toast } from "sonner";
import {
  fileShareCreate,
  fileShareImport,
  fileSharePlan,
  fileSharePreview,
  onFileShareProgress,
} from "../../api/mods";
import type { BundlePhase, FileShare, SharePlan } from "../../types";
import { formatBytes } from "../../lib/mods";
import { copyText } from "../../lib/clipboard";
import { useT, type TFunc } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";

function phaseLabel(phase: BundlePhase, t: TFunc): string {
  switch (phase) {
    case "bundling":
      return t("share.phasePacking");
    case "uploading":
      return t("share.phaseUploading");
    case "downloading":
      return t("share.phaseDownloading");
    case "installing":
      return t("share.phaseInstalling");
    case "done":
      return t("common.done");
  }
}

/** The files a share carries, listed with the folder each one goes back into. */
function ItemList({ items }: { items: { rel: string; name: string; size: number }[] }) {
  return (
    <div className="max-h-40 overflow-y-auto rounded-lg border border-white/[0.07] bg-card/40">
      {items.map((item) => (
        <div
          key={item.rel}
          className="flex items-baseline gap-2 border-b border-white/[0.04] px-2.5 py-1.5 text-[12px] last:border-b-0"
        >
          <span className="truncate font-semibold">{item.name}</span>
          <span className="truncate text-[11px] text-faint">{item.rel}</span>
          <span className="ml-auto flex-none text-[11px] text-muted-foreground">
            {formatBytes(item.size)}
          </span>
        </div>
      ))}
    </div>
  );
}

/**
 * Share the picked paths. `paths` non-null opens the dialog; the plan is worked out
 * up front so the size is on screen *before* anything is uploaded.
 */
export function ShareDialog({
  paths,
  onClose,
}: {
  paths: string[] | null;
  onClose: () => void;
}) {
  const t = useT();
  const [plan, setPlan] = useState<SharePlan | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<BundlePhase | null>(null);

  useEffect(() => {
    if (!paths) return;
    setPlan(null);
    setCode(null);
    setCopied(false);
    setPhase(null);
    let cancelled = false;
    fileSharePlan(paths)
      .then((p) => !cancelled && setPlan(p))
      .catch((e) => !cancelled && toast.error(String(e).replace(/^Error:\s*/, "")));
    return () => {
      cancelled = true;
    };
  }, [paths]);

  const create = useCallback(async () => {
    if (!paths) return;
    setBusy(true);
    setPhase("bundling");
    const unlisten = await onFileShareProgress((p) => setPhase(p.phase));
    try {
      const c = await fileShareCreate(paths);
      setCode(c);
      setCopied(false);
      // Straight to the clipboard: the code exists to be pasted somewhere, and a player
      // who has waited out an upload shouldn't have to click again to collect it.
      if (await copyText(c)) {
        setCopied(true);
        toast.success(t("share.uploadedCopied"));
      } else {
        toast.success(t("share.uploaded"));
      }
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      unlisten();
      setBusy(false);
      setPhase(null);
    }
  }, [paths, t]);

  const count = plan?.items.length ?? 0;

  return (
    <Dialog open={!!paths} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("share.title")}</DialogTitle>
          <DialogDescription>
            {code ? t("share.hintDone") : t("share.hint")}
          </DialogDescription>
        </DialogHeader>

        {plan && count > 0 && <ItemList items={plan.items} />}

        {plan && count === 0 && (
          <p className="flex items-start gap-1.5 text-[12px] text-muted-foreground">
            <AlertTriangle className="mt-px size-3.5 flex-none" />
            {t("share.nothingToShare")}
          </p>
        )}

        {plan && plan.skipped.length > 0 && (
          <p className="flex items-start gap-1.5 text-[11.5px] text-amber-500">
            <AlertTriangle className="mt-px size-3.5 flex-none" />
            <span>
              {t("share.skipped", {
                count: plan.skipped.length,
                reason: plan.skipped[0].reason,
              })}
            </span>
          </p>
        )}

        {code ? (
          <textarea
            readOnly
            value={code}
            onFocus={(e) => e.currentTarget.select()}
            className="h-24 w-full resize-none rounded-lg border border-input bg-transparent p-2.5 font-mono text-[11px] leading-snug"
          />
        ) : (
          plan &&
          count > 0 && (
            <p className="text-[11px] text-faint">{t("presets.shareWarning")}</p>
          )
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("common.close")}
          </Button>
          {code ? (
            <Button
              onClick={async () => {
                if (await copyText(code)) {
                  setCopied(true);
                  toast.success(t("share.copied"));
                } else {
                  toast.error(t("presets.copyFailed"));
                }
              }}
            >
              {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
              {copied ? t("modDetail.copied") : t("share.copyCode")}
            </Button>
          ) : (
            <Button disabled={busy || !plan || count === 0} onClick={() => void create()}>
              {busy ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <UploadCloud className="size-4" />
              )}
              {busy && phase
                ? phaseLabel(phase, t)
                : t("share.createCode", {
                    count,
                    size: formatBytes(plan?.totalSize ?? 0),
                  })}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Paste a `MXBS1-` code and install what it carries. */
export function ImportShareDialog({
  open,
  initialCode = "",
  onClose,
  onImported,
}: {
  open: boolean;
  /** Prefilled when the code arrived on its own — pasted into the window, say — rather
   *  than from someone opening this to type one in. */
  initialCode?: string;
  onClose: () => void;
  onImported: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(initialCode);
  const [preview, setPreview] = useState<FileShare | null>(null);
  const [previewErr, setPreviewErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<BundlePhase | null>(null);

  // Each opening starts from whatever it was opened with — a pasted code, or nothing.
  useEffect(() => {
    setText(open ? initialCode : "");
    setPreview(null);
    setPreviewErr(null);
    setPhase(null);
  }, [open, initialCode]);

  useEffect(() => {
    const code = text.trim();
    if (!code) {
      setPreview(null);
      setPreviewErr(null);
      return;
    }
    let cancelled = false;
    fileSharePreview(code)
      .then((s) => {
        if (cancelled) return;
        setPreview(s);
        setPreviewErr(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setPreview(null);
        setPreviewErr(String(e).replace(/^Error:\s*/, ""));
      });
    return () => {
      cancelled = true;
    };
  }, [text]);

  const install = useCallback(async () => {
    if (!preview) return;
    setBusy(true);
    setPhase("downloading");
    const unlisten = await onFileShareProgress((p) => setPhase(p.phase));
    try {
      const done = await fileShareImport(text.trim());
      toast.success(t("share.installed", { count: done.items.length }));
      onImported();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      unlisten();
      setBusy(false);
      setPhase(null);
    }
  }, [preview, text, onImported, t]);

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("share.importTitle")}</DialogTitle>
          <DialogDescription>{t("share.importBody")}</DialogDescription>
        </DialogHeader>

        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="MXBS1-…"
          className="h-24 w-full resize-none rounded-lg border border-input bg-transparent p-2.5 font-mono text-[11px] leading-snug placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        />

        {previewErr && text.trim() && (
          <p className="flex items-start gap-1.5 text-[12px] text-destructive">
            <AlertTriangle className="mt-px size-3.5 flex-none" />
            {previewErr}
          </p>
        )}

        {preview && (
          <>
            <ItemList items={preview.items} />
            <p className="flex items-start gap-1.5 text-[11.5px] text-emerald-500">
              <Share2 className="mt-px size-3.5 flex-none" />
              <span>
                {t("share.downloadNotice", {
                  size: formatBytes(preview.totalSize),
                  host: preview.bundle.host,
                })}
              </span>
            </p>
          </>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("common.cancel")}
          </Button>
          <Button disabled={!preview || busy} onClick={() => void install()}>
            {busy ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Download className="size-4" />
            )}
            {busy && phase ? phaseLabel(phase, t) : t("share.install")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
