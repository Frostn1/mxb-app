import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Lock, ShieldCheck, FileUp, Loader2, Copy, Check, UserCheck, WifiOff } from "lucide-react";
import { Button } from "../ui/button";
import HelpHint from "@/Components/ui/help-hint";
import { cn } from "@/lib/utils";
import { formatBytes } from "../../lib/mods";
import {
  mxbsecureLock,
  mxbsecureVerify,
  secureSteamId,
  mxbsecureProvision,
  mxbsecureOpenOffline,
  type SecureLockOutcome,
} from "../../api/mods";
import { useT } from "../../i18n/context";

/**
 * The mxbsecure Lock tab — experimental.
 *
 * Three steps: lock a file into a `.mxbsecure` blob, verify it unlocks with the key, then
 * bind it to your Steam account and open it *offline* — the app seals the key to the live
 * Steam ID, then decrypts with no server and confirms it still matches. A copy on another
 * account gets nothing. The in-game DLL is a separate, deeper test.
 */
const Secure = () => {
  const t = useT();
  const [src, setSrc] = useState<string | null>(null);
  const [busy, setBusy] = useState<"locking" | "verifying" | null>(null);
  const [locked, setLocked] = useState<SecureLockOutcome | null>(null);
  const [verified, setVerified] = useState<boolean | null>(null);
  const [copied, setCopied] = useState(false);
  const [steamId, setSteamId] = useState<string | null | undefined>(undefined);
  const [provisioning, setProvisioning] = useState(false);
  const [provisioned, setProvisioned] = useState(false);
  const [offlineOk, setOfflineOk] = useState<boolean | null>(null);
  const [openingOffline, setOpeningOffline] = useState(false);

  useEffect(() => {
    secureSteamId().then(setSteamId).catch(() => setSteamId(null));
  }, []);

  const provision = async () => {
    if (!locked) return;
    setProvisioning(true);
    setOfflineOk(null);
    try {
      const out = await mxbsecureProvision(locked.blobPath, locked.key);
      setProvisioned(true);
      setSteamId(out.steamId);
      toast.success(t("secure.provisioned", { id: out.steamId }));
    } catch (e) {
      toast.error(t("secure.provisionFailed"), { description: String(e) });
    }
    setProvisioning(false);
  };

  const openOffline = async () => {
    if (!locked || !src) return;
    setOpeningOffline(true);
    try {
      const ok = await mxbsecureOpenOffline(locked.blobPath, src);
      setOfflineOk(ok);
      if (ok) toast.success(t("secure.offlineOk"));
      else toast.error(t("secure.offlineBad"));
    } catch (e) {
      setOfflineOk(false);
      toast.error(t("secure.offlineFailed"), { description: String(e) });
    }
    setOpeningOffline(false);
  };

  const pick = async () => {
    const chosen = await openDialog({ multiple: false, directory: false });
    if (typeof chosen === "string") {
      setSrc(chosen);
      setLocked(null);
      setVerified(null);
    }
  };

  const lock = async () => {
    if (!src) return;
    setBusy("locking");
    setVerified(null);
    try {
      const outcome = await mxbsecureLock(src);
      setLocked(outcome);
      setProvisioned(false);
      setOfflineOk(null);
      toast.success(t("secure.locked"));
    } catch (e) {
      toast.error(t("secure.lockFailed"), { description: String(e) });
    }
    setBusy(null);
  };

  const verify = async () => {
    if (!locked || !src) return;
    setBusy("verifying");
    try {
      const ok = await mxbsecureVerify(locked.blobPath, locked.key, src);
      setVerified(ok);
      if (ok) toast.success(t("secure.verifiedOk"));
      else toast.error(t("secure.verifiedBad"));
    } catch (e) {
      setVerified(false);
      toast.error(t("secure.verifyFailed"), { description: String(e) });
    }
    setBusy(null);
  };

  const copyKey = async () => {
    if (!locked) return;
    await navigator.clipboard.writeText(locked.key);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const fileName = src?.split(/[\\/]/).pop() ?? "";

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
        <p className="mb-5 text-[13px] leading-relaxed text-muted-foreground">
          {t("secure.intro")}
        </p>

        {/* Step 1 — pick and lock */}
        <div className="rounded-xl border border-white/[0.07] p-4">
          <div className="flex items-center gap-2">
            <span className="flex size-5 items-center justify-center rounded-full bg-white/[0.06] text-[11px] font-semibold">
              1
            </span>
            <h2 className="text-[13.5px] font-semibold">{t("secure.step1")}</h2>
          </div>

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" onClick={() => void pick()} disabled={busy !== null}>
              <FileUp className="size-3.5" /> {t("secure.pick")}
            </Button>
            {src && (
              <span className="truncate text-[12.5px] text-muted-foreground" title={src}>
                {fileName}
              </span>
            )}
          </div>

          <Button
            className="mt-3"
            size="sm"
            disabled={!src || busy !== null}
            onClick={() => void lock()}
          >
            {busy === "locking" ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Lock className="size-3.5" />
            )}
            {t("secure.lock")}
          </Button>
        </div>

        {/* The result: blob + the key, shown once */}
        {locked && (
          <div className="mt-4 rounded-xl border border-white/[0.07] p-4">
            <h2 className="text-[13.5px] font-semibold">{t("secure.result")}</h2>
            <dl className="mt-2 space-y-1.5 text-[12.5px]">
              <Row label={t("secure.blob")} value={locked.blobPath} mono />
              <Row
                label={t("secure.size")}
                value={t("secure.sizeValue", {
                  plain: formatBytes(locked.plainBytes),
                  blob: formatBytes(locked.blobBytes),
                })}
              />
              <Row label={t("secure.assetId")} value={locked.assetId} mono />
              <div className="flex items-start gap-2 py-0.5">
                <dt className="w-24 flex-none text-muted-foreground">{t("secure.key")}</dt>
                <dd className="flex min-w-0 flex-1 items-center gap-1.5">
                  <code className="truncate font-mono text-[11.5px]">{locked.key}</code>
                  <button
                    onClick={() => void copyKey()}
                    title={t("secure.copyKey")}
                    className="flex-none text-muted-foreground hover:text-foreground"
                  >
                    {copied ? <Check className="size-3.5 text-success" /> : <Copy className="size-3.5" />}
                  </button>
                </dd>
              </div>
            </dl>
            <p className="mt-2 text-[11.5px] leading-relaxed text-muted-foreground">
              {t("secure.keyNote")}
            </p>
          </div>
        )}

        {/* Step 2 — verify it unlocks */}
        {locked && (
          <div className="mt-4 rounded-xl border border-white/[0.07] p-4">
            <div className="flex items-center gap-2">
              <span className="flex size-5 items-center justify-center rounded-full bg-white/[0.06] text-[11px] font-semibold">
                2
              </span>
              <h2 className="text-[13.5px] font-semibold">{t("secure.step2")}</h2>
            </div>
            <p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">
              {t("secure.step2Desc")}
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => void verify()}>
                {busy === "verifying" ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <ShieldCheck className="size-3.5" />
                )}
                {t("secure.verify")}
              </Button>
              {verified !== null && (
                <span
                  className={cn(
                    "flex items-center gap-1.5 text-[12.5px] font-medium",
                    verified ? "text-success" : "text-destructive",
                  )}
                >
                  {verified ? <Check className="size-4" /> : null}
                  {verified ? t("secure.unlockedMatches") : t("secure.unlockedMismatch")}
                </span>
              )}
            </div>
          </div>
        )}

        {/* Step 3 — bind to the Steam account and play offline */}
        {locked && (
          <div className="mt-4 rounded-xl border border-white/[0.07] p-4">
            <div className="flex items-center gap-2">
              <span className="flex size-5 items-center justify-center rounded-full bg-white/[0.06] text-[11px] font-semibold">
                3
              </span>
              <h2 className="text-[13.5px] font-semibold">{t("secure.step3")}</h2>
            </div>
            <p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">
              {t("secure.step3Desc")}
            </p>

            {steamId === null ? (
              <p className="mt-3 text-[12px] text-warning">{t("secure.noSteam")}</p>
            ) : (
              <>
                {steamId && (
                  <p className="mt-2 text-[11.5px] text-muted-foreground">
                    {t("secure.steamAccount")}{" "}
                    <code className="font-mono text-foreground/80">{steamId}</code>
                  </p>
                )}
                <div className="mt-3 flex flex-wrap items-center gap-3">
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={provisioning || steamId === undefined}
                    onClick={() => void provision()}
                  >
                    {provisioning ? (
                      <Loader2 className="size-3.5 animate-spin" />
                    ) : (
                      <UserCheck className="size-3.5" />
                    )}
                    {t("secure.provision")}
                  </Button>
                  {provisioned && (
                    <span className="flex items-center gap-1.5 text-[12.5px] font-medium text-success">
                      <Check className="size-4" /> {t("secure.provisionedOk")}
                    </span>
                  )}
                </div>

                {provisioned && (
                  <div className="mt-3 flex flex-wrap items-center gap-3 border-t border-white/[0.05] pt-3">
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={openingOffline}
                      onClick={() => void openOffline()}
                    >
                      {openingOffline ? (
                        <Loader2 className="size-3.5 animate-spin" />
                      ) : (
                        <WifiOff className="size-3.5" />
                      )}
                      {t("secure.openOffline")}
                    </Button>
                    {offlineOk !== null && (
                      <span
                        className={cn(
                          "flex items-center gap-1.5 text-[12.5px] font-medium",
                          offlineOk ? "text-success" : "text-destructive",
                        )}
                      >
                        {offlineOk ? <Check className="size-4" /> : null}
                        {offlineOk ? t("secure.offlineMatches") : t("secure.offlineMismatch")}
                      </span>
                    )}
                  </div>
                )}
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

const Row = ({ label, value, mono }: { label: string; value: string; mono?: boolean }) => (
  <div className="flex items-start gap-2 py-0.5">
    <dt className="w-24 flex-none text-muted-foreground">{label}</dt>
    <dd className={cn("min-w-0 flex-1 break-all", mono && "font-mono text-[11.5px]")} title={value}>
      {value}
    </dd>
  </div>
);

export default Secure;
