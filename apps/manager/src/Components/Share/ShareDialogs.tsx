/**
 * Sharing installed content, and taking someone else's.
 *
 * The preset share in the Presets tab hands over a *look*. This hands over the files —
 * a track, a paint, a folder of both — as a one-line code that installs them where the
 * sender had them. Same upload, same slicing past one part; see `src-tauri/src/fileshare.rs`.
 */
import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  Check,
  Cloud,
  Copy,
  Download,
  Loader2,
  Share2,
  UploadCloud,
} from "lucide-react";
import { toast } from "sonner";
import {
  fileShareCreate,
  fileShareImport,
  fileSharePlan,
  fileSharePreview,
  liveSharePreview,
  liveSharePublish,
  liveShareSubscribe,
  onFileShareProgress,
} from "@frost/shared/api/mods";
import type { BundlePhase, SharePlan, SharePreview } from "@frost/shared/types";
import { isLiveCode } from "../../lib/liveshare";
import { Switch } from "@frost/shared/Components/ui/switch";
import { formatBytes } from "@frost/shared/lib/mods";
import { copyText } from "../../lib/clipboard";
import { useT, type TFunc, type TKey } from "@/i18n";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@frost/shared/Components/ui/dialog";

function phaseLabel(phase: BundlePhase, t: TFunc<TKey>): string {
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

/**
 * The files a share carries, listed with the folder each one goes back into.
 *
 * `replaces` names the ones already on this machine. An import overwrites without asking,
 * so the row says so before the download rather than after.
 */
function ItemList({
  items,
  replaces,
}: {
  items: { rel: string; name: string; size: number }[];
  replaces?: Set<string>;
}) {
  const t = useT();
  return (
    <div className="max-h-40 overflow-y-auto rounded-lg border border-white/[0.07] bg-card/40">
      {items.map((item) => (
        <div
          key={item.rel}
          className="flex items-baseline gap-2 border-b border-white/[0.04] px-2.5 py-1.5 text-[12px] last:border-b-0"
        >
          <span className="truncate font-semibold">{item.name}</span>
          <span className="truncate text-[11px] text-faint">{item.rel}</span>
          {replaces?.has(item.rel) && (
            <span className="flex-none text-[10.5px] font-semibold uppercase tracking-wide text-warning">
              {t("share.replacesTag")}
            </span>
          )}
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
  // Off by default: a plain code needs no server and keeps working if ours is down, so it
  // stays the thing that happens when nobody asked for anything else.
  const [live, setLive] = useState(false);

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
      const c = live ? (await liveSharePublish(paths)).code : await fileShareCreate(paths);
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
  }, [paths, live, t]);

  const count = plan?.items.length ?? 0;

  return (
    <Dialog open={!!paths} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("share.title")}</DialogTitle>
          <DialogDescription>
            {code ? (live ? t("share.hintDoneLive") : t("share.hintDone")) : t("share.hint")}
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

        {/* An input for a live code, a textarea for a plain one — the element matches the
            value rather than being restyled into it. A live code is eight characters on one
            line; a `MXBS1-` code is a base64 wall that genuinely needs to wrap. Sizing a
            textarea down to one line leaves it scrolling its own single line. */}
        {code ? (
          live ? (
            <input
              readOnly
              value={code}
              onFocus={(e) => e.currentTarget.select()}
              className="w-full rounded-lg border border-input bg-transparent px-2.5 py-2 text-center font-mono text-[15px] tracking-[0.2em]"
            />
          ) : (
            <textarea
              readOnly
              value={code}
              onFocus={(e) => e.currentTarget.select()}
              className="h-24 w-full resize-none rounded-lg border border-input bg-transparent p-2.5 font-mono text-[11px] leading-snug"
            />
          )
        ) : (
          <label className="flex cursor-pointer items-start gap-2.5 rounded-lg border border-white/[0.07] bg-card/40 p-2.5">
            <Switch checked={live} onCheckedChange={setLive} disabled={busy} className="mt-px" />
            <span className="min-w-0">
              <span className="flex items-center gap-1.5 text-[12.5px] font-semibold">
                <Cloud className="size-3.5 flex-none" />
                {t("share.keepUpdated")}
              </span>
              <span className="mt-0.5 block text-[11.5px] leading-snug text-muted-foreground">
                {t("share.keepUpdatedHint")}
              </span>
            </span>
          </label>
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

/**
 * Paste a share code and install what it carries.
 *
 * Two kinds arrive in the same box. A `MXBS1-` code is decoded on the spot; a `MXBL1-` one
 * has to be fetched, because what it points at lives on the control plane and changes. The
 * preview and the install differ only in which of those two the code is routed to — every
 * other guard, and the whole download, is shared.
 */
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
  const [preview, setPreview] = useState<SharePreview | null>(null);
  const [previewErr, setPreviewErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<BundlePhase | null>(null);
  const live = isLiveCode(text);

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
    // A live code costs a round trip to preview, so this is debounced — the box is typed
    // into character by character and a request per keystroke is a request per keystroke.
    const run = isLiveCode(code) ? liveSharePreview : fileSharePreview;
    const delay = isLiveCode(code) ? 350 : 0;
    const timer = setTimeout(() => {
    run(code)
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
    }, delay);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [text]);

  const install = useCallback(async () => {
    if (!preview) return;
    setBusy(true);
    setPhase("downloading");
    const unlisten = await onFileShareProgress((p) => setPhase(p.phase));
    try {
      const code = text.trim();
      const done = live ? await liveShareSubscribe(code) : await fileShareImport(code);
      toast.success(
        live
          ? t("share.subscribed", { count: done.items.length })
          : t("share.installed", { count: done.items.length }),
      );
      onImported();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      unlisten();
      setBusy(false);
      setPhase(null);
    }
  }, [preview, text, live, onImported, t]);

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
          placeholder="MXBS1-… / MXBL1-…"
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
            <ItemList items={preview.items} replaces={new Set(preview.existing)} />
            {preview.existing.length > 0 && (
              <p className="flex items-start gap-1.5 text-[11.5px] text-warning">
                <AlertTriangle className="mt-px size-3.5 flex-none" />
                <span>
                  {t("share.willReplace", { count: preview.existing.length })}
                </span>
              </p>
            )}
            <p className="flex items-start gap-1.5 text-[11.5px] text-emerald-500">
              <Share2 className="mt-px size-3.5 flex-none" />
              <span>
                {t("share.downloadNotice", {
                  size: formatBytes(preview.totalSize),
                  host: preview.bundle.host,
                })}
              </span>
            </p>
            {live && (
              <p className="flex items-start gap-1.5 text-[11.5px] text-sky-400">
                <Cloud className="mt-px size-3.5 flex-none" />
                {t("share.liveNotice")}
              </p>
            )}
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
            {busy && phase ? phaseLabel(phase, t) : live ? t("share.follow") : t("share.install")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
