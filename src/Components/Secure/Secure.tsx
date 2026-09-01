import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Lock, Loader2, FileUp, Check, X } from "lucide-react";
import { Button } from "../ui/button";
import HelpHint from "@/Components/ui/help-hint";
import { cn } from "@/lib/utils";
import { mxbsecureGenerate, type SecureGenerateOutcome } from "../../api/mods";
import { useT } from "../../i18n/context";

/**
 * The mxbsecure tab — protect tracks for a buyer.
 *
 * Pick one or more tracks and type the buyer's Steam ID. For each track the app writes two
 * files beside it: `<track>.mxbsecure` (the encrypted copy) and `<track>.mxbsecure.mxbkey`
 * (the key sealed to that Steam ID). The original is never touched. The buyer drops both files
 * into their tracks folder — the injected client lists the track and decrypts it on load, only
 * on the machine signed into that Steam account, offline.
 */
const Secure = () => {
  const t = useT();
  const [steamId, setSteamId] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<SecureGenerateOutcome[]>([]);

  const steamIdOk = /^\d{17}$/.test(steamId.trim());

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

  const generate = async () => {
    if (!steamIdOk) {
      toast.error(t("secure.badSteamId"));
      return;
    }
    if (!files.length) return;
    setBusy(true);
    setResults([]);
    const done: SecureGenerateOutcome[] = [];
    for (const path of files) {
      try {
        done.push(await mxbsecureGenerate(path, steamId.trim()));
      } catch (e) {
        toast.error(t("secure.genFail", { name: path.split(/[\\/]/).pop() ?? path }), {
          description: String(e),
        });
      }
    }
    setResults(done);
    if (done.length) {
      toast.success(
        t("secure.genOk", { ok: done.length, total: files.length, id: steamId.trim() }),
      );
    }
    setBusy(false);
  };

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <header className="flex flex-none items-center gap-1.5 px-7 pb-2 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">{t("nav.secure")}</h1>
        <HelpHint title={t("nav.secure")} description={t("secure.help")} />
        <span className="ml-1 rounded-full border border-warning/40 bg-warning/[0.08] px-2 py-0.5 text-[10.5px] font-medium text-warning">
          {t("secure.experimental")}
        </span>
      </header>

      <div className="mx-auto w-full max-w-2xl px-7 pb-10">
        <div className="rounded-xl border border-primary/30 bg-primary/[0.04] p-5">
          <div className="flex items-center gap-2">
            <Lock className="size-4 text-primary" />
            <h2 className="text-[14px] font-semibold">{t("secure.genTitle")}</h2>
          </div>
          <p className="mt-1.5 text-[12.5px] leading-relaxed text-muted-foreground">
            {t("secure.genDesc")}
          </p>

          {/* Buyer's Steam ID */}
          <label className="mt-4 block text-[12px] font-medium text-foreground/90">
            {t("secure.steamIdLabel")}
          </label>
          <input
            value={steamId}
            onChange={(e) => setSteamId(e.target.value.replace(/[^\d]/g, "").slice(0, 17))}
            inputMode="numeric"
            placeholder={t("secure.steamIdPlaceholder")}
            className={cn(
              "mt-1.5 w-full rounded-lg border bg-background/60 px-3 py-2 font-mono text-[13px] outline-none transition-colors",
              steamId.length === 0
                ? "border-white/[0.1] focus:border-primary/50"
                : steamIdOk
                  ? "border-success/50"
                  : "border-destructive/50",
            )}
          />
          <p className="mt-1.5 text-[11px] text-muted-foreground">{t("secure.steamIdHint")}</p>

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
                  className="flex items-center gap-2 rounded-md bg-white/[0.03] px-2.5 py-1.5"
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
            disabled={busy || !steamIdOk || files.length === 0}
            onClick={() => void generate()}
          >
            {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Lock className="size-3.5" />}
            {t("secure.generate")}
          </Button>
        </div>

        {/* Results — the two files per track, ready to send */}
        {results.length > 0 && (
          <div className="mt-4 rounded-xl border border-white/[0.07] p-4">
            <div className="flex items-center gap-2">
              <Check className="size-4 text-success" />
              <h2 className="text-[13.5px] font-semibold">{t("secure.genResult")}</h2>
            </div>
            <p className="mt-1.5 text-[12px] leading-relaxed text-muted-foreground">
              {t("secure.buyerNote")}
            </p>
            <ul className="mt-3 space-y-3">
              {results.map((r) => (
                <li key={r.blobPath} className="border-t border-white/[0.05] pt-3 first:border-0 first:pt-0">
                  <p className="text-[12.5px] font-medium">{r.gameName}</p>
                  <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground" title={r.blobPath}>
                    {r.blobPath}
                  </p>
                  <p className="break-all font-mono text-[11px] text-muted-foreground" title={r.mxbkeyPath}>
                    {r.mxbkeyPath}
                  </p>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
};

export default Secure;
