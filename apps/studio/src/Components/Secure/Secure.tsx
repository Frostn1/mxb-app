import { useState } from "react";
import { Card } from "@frost/shared/Components/ui/card";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Lock, Loader2, FileUp, Check, X, Copy } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { ContextBarRight } from "../Shell/ContextBar";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import { mxbsecureGenerate, type SecureGenerateOutcome } from "@frost/shared/api/mods";
import { useT } from "@/i18n";

/**
 * The mxbsecure tab — pack tracks for distribution.
 *
 * Pick one or more tracks. For each, the app writes `<track>.mxbsecure` (the encrypted copy)
 * beside it and returns an asset id and a content key. The original is never touched. It does
 * NOT seal a per-buyer key here: a key sealed for a buyer on another machine can't be
 * machine-bound, so it would be portable — a shared file plus the buyer's public Steam ID would
 * open it anywhere. Instead the creator registers the asset id + content key with the store, and
 * a buyer who owns the track provisions on their own machine (the manager's unlock step), which
 * DPAPI-binds the key so a copy is useless.
 */
const Secure = () => {
  const t = useT();
  const [files, setFiles] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<SecureGenerateOutcome[]>([]);

  const pick = async () => {
    const chosen = await openDialog({ multiple: true, directory: false });
    if (!chosen) return;
    const list = (Array.isArray(chosen) ? chosen : [chosen]).filter(
      (p): p is string => typeof p === "string",
    );
    if (list.length) {
      setFiles(list);
      setResults([]);
    }
  };

  const removeFile = (path: string) => {
    setFiles((f) => f.filter((p) => p !== path));
    setResults([]);
  };

  const copy = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(t("secure.copied"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const generate = async () => {
    if (!files.length) return;
    setBusy(true);
    setResults([]);
    const done: SecureGenerateOutcome[] = [];
    for (const path of files) {
      try {
        done.push(await mxbsecureGenerate(path));
      } catch (e) {
        toast.error(t("secure.genFail", { name: path.split(/[\\/]/).pop() ?? path }), {
          description: String(e),
        });
      }
    }
    setResults(done);
    if (done.length) {
      toast.success(t("secure.genOk", { ok: done.length, total: files.length }));
    }
    setBusy(false);
  };

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <ContextBarRight>
        <span className="u-skew border border-warning/40 bg-warning/[0.08] px-2 py-0.5">
          <span className="u-unskew block font-cond text-[10.5px] font-semibold uppercase tracking-[0.14em] text-warning">
            {t("secure.experimental")}
          </span>
        </span>
        <HelpHint title={t("nav.secure")} description={t("secure.help")} />
      </ContextBarRight>

      <div className="mx-auto w-full max-w-2xl px-4 pb-10">
        <Card data-raised className="rounded-lg border border-primary/30 bg-primary/[0.04] p-5">
          <div className="flex items-center gap-2">
            <Lock className="size-4 text-primary" />
            <h2 className="text-[14px] font-semibold">{t("secure.genTitle")}</h2>
          </div>
          <p className="mt-1.5 text-[12.5px] leading-relaxed text-muted-foreground">
            {t("secure.genDesc")}
          </p>

          {/* Track selection */}
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" onClick={() => void pick()} disabled={busy}>
              <FileUp className="size-3.5" /> {t("secure.pickTracks")}
            </Button>
            {files.length > 0 && (
              <span className="text-[12px] text-muted-foreground">
                {t("secure.selected", { count: files.length })}
              </span>
            )}
          </div>

          {files.length > 0 && (
            <ul className="mt-3 space-y-1">
              {files.map((path) => (
                <li
                  key={path}
                  className="flex items-center gap-2 rounded-md bg-foreground/[0.03] px-2.5 py-1.5"
                >
                  <span className="min-w-0 flex-1 truncate text-[12px]" title={path}>
                    {path.split(/[\\/]/).pop()}
                  </span>
                  {!busy && (
                    <button
                      onClick={() => removeFile(path)}
                      className="flex-none text-muted-foreground hover:text-foreground"
                      title={t("common.delete")}
                    >
                      <X className="size-3.5" />
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}

          <Button
            className="mt-4"
            size="sm"
            disabled={busy || files.length === 0}
            onClick={() => void generate()}
          >
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Lock className="size-3.5" />}
            {t("secure.generate")}
          </Button>
        </Card>

        {/* Results — the blob to distribute, and the asset id + key to register with the store */}
        {results.length > 0 && (
          <Card className="mt-4 p-4">
            <div className="flex items-center gap-2">
              <Check className="size-4 text-success" />
              <h2 className="text-[13.5px] font-semibold">{t("secure.genResult")}</h2>
            </div>
            <p className="mt-1.5 text-[12px] leading-relaxed text-muted-foreground">
              {t("secure.registerNote")}
            </p>
            <ul className="mt-3 space-y-3">
              {results.map((r) => (
                <li key={r.blobPath} className="border-t border-border pt-3 first:border-0 first:pt-0">
                  <p className="text-[12.5px] font-medium">{r.gameName}</p>
                  <p
                    className="mt-1 break-all font-mono text-[11px] text-muted-foreground"
                    title={r.blobPath}
                  >
                    {r.blobPath}
                  </p>
                  <div className="mt-2 space-y-1.5">
                    <CopyRow label={t("secure.assetId")} value={r.assetId} onCopy={copy} copyTitle={t("secure.copy")} />
                    <CopyRow label={t("secure.contentKey")} value={r.contentKey} onCopy={copy} copyTitle={t("secure.copy")} />
                  </div>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    </div>
  );
};

/** A labelled monospace value with a copy button — the asset id and the content key. */
const CopyRow = ({
  label,
  value,
  onCopy,
  copyTitle,
}: {
  label: string;
  value: string;
  onCopy: (v: string) => void;
  copyTitle: string;
}) => (
  <div className="flex items-center gap-2">
    <span className="w-24 flex-none text-[11px] text-muted-foreground">{label}</span>
    <code className="min-w-0 flex-1 truncate rounded bg-foreground/[0.05] px-2 py-1 text-[11px]" title={value}>
      {value}
    </code>
    <button
      onClick={() => onCopy(value)}
      className="flex-none text-muted-foreground hover:text-foreground"
      title={copyTitle}
    >
      <Copy className="size-3.5" />
    </button>
  </div>
);

export default Secure;
