import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { Button } from "../Components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../Components/ui/alert-dialog";
import { useT, type BaseTKey } from "../i18n/context";
import { type Permission } from "./permissions";
import { isOverDropTarget } from "./dropTargets";
import {
  checkPublish,
  backupAuthorKey,
  myListings,
  publishExtension,
  requestVerified,
  restoreAuthorKey,
  type MyListing,
  type PublishCheck,
} from "./publishApi";

const labels: Record<Permission, BaseTKey> = {
  "mods.read": "ext.perm.mods",
  "servers.read": "ext.perm.servers",
  "friends.presence": "ext.perm.friends",
  "game.state": "ext.perm.game",
  "presets.read": "ext.perm.presets",
  "coach.read": "ext.perm.coach",
  "studio.launch": "ext.perm.studio",
  network: "ext.perm.network",
  openExternal: "ext.perm.openExternal",
  "mcp.tools": "ext.perm.mcp",
};

/** Gives extension authors a preview-first publishing flow in the Build tab. */
export function PublishSection() {
  const t = useT();
  const [path, setPath] = useState<string | null>(null);
  const [check, setCheck] = useState<PublishCheck | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<"listed" | "review" | null>(null);
  const [doneId, setDoneId] = useState("");
  const [doneTier, setDoneTier] = useState("community");
  const [doneVersion, setDoneVersion] = useState("");
  const [verificationRequested, setVerificationRequested] = useState(false);
  const [verifyError, setVerifyError] = useState<{
    id: string;
    target: "done" | "listing";
    message: string;
  } | null>(null);
  const [mine, setMine] = useState<MyListing[]>([]);
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyMessage, setKeyMessage] = useState("");
  const [keyError, setKeyError] = useState("");
  const [restorePath, setRestorePath] = useState<string | null>(null);

  const refresh = useCallback(() => {
    void myListings().then((result) => setMine(result.extensions), () => setMine([]));
  }, []);
  const inspect = useCallback(async (nextPath: string) => {
    setPath(nextPath);
    setCheck(null);
    setError("");
    setDone(null);
    setBusy(true);
    try {
      setCheck(await checkPublish(nextPath));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(refresh, [refresh]);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop" || !event.payload.paths[0]) return;
      if (isOverDropTarget(event.payload.position, "ext-publish")) {
        void inspect(event.payload.paths[0]);
      }
    }).then((stop) => cancelled ? stop() : unlisten = stop);
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [inspect]);

  const choose = async (folder: boolean) => {
    const selected = await open({
      directory: folder,
      multiple: false,
      filters: folder ? undefined : [{ name: "MXB extension", extensions: ["mxbext"] }],
    });
    if (typeof selected === "string") void inspect(selected);
  };
  const publish = async () => {
    if (!path) return;
    setBusy(true);
    try {
      const result = await publishExtension(path);
      setDoneVersion(check?.preview.version ?? "");
      setDone(result.status);
      setDoneId(result.id);
      setDoneTier(result.tier);
      setCheck(null);
      refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };
  const verify = async (id: string, target: "done" | "listing") => {
    setVerifyError(null);
    try {
      await requestVerified(id);
      if (id === doneId) setVerificationRequested(true);
      refresh();
    } catch (reason) {
      setVerifyError({ id, target, message: String(reason) });
    }
  };
  const reset = () => {
    setPath(null);
    setCheck(null);
    setError("");
    setDone(null);
    setDoneId("");
    setDoneVersion("");
    setVerificationRequested(false);
    setVerifyError(null);
  };
  const status = (listing: MyListing) => {
    if (listing.review?.state === "rejected") {
      return listing.review.note.trim()
        ? t("ext.publish.rejected", { note: listing.review.note })
        : t("ext.publish.rejectedNoNote");
    }
    if (listing.review?.state === "pending") {
      return listing.verified
        ? t("ext.publish.verifiedUpdateReview", { version: listing.review.version })
        : t("ext.publish.inReview", { version: listing.review.version });
    }
    return listing.verified ? t("ext.publish.verified") : t("ext.publish.community");
  };

  const backupKey = async () => {
    const dest = await save({
      defaultPath: "mxb-author-key.pem",
      filters: [{ name: "PEM", extensions: ["pem"] }],
    });
    if (typeof dest !== "string") return;
    setKeyBusy(true);
    setKeyMessage("");
    setKeyError("");
    try {
      await backupAuthorKey(dest);
      setKeyMessage(t("ext.publish.keySaved"));
    } catch (reason) {
      setKeyError(String(reason));
    } finally {
      setKeyBusy(false);
    }
  };
  const chooseRestore = async () => {
    const src = await open({
      multiple: false,
      filters: [{ name: "PEM", extensions: ["pem"] }],
    });
    if (typeof src === "string") setRestorePath(src);
  };
  const restoreKey = async () => {
    if (!restorePath) return;
    setKeyBusy(true);
    setKeyMessage("");
    setKeyError("");
    try {
      await restoreAuthorKey(restorePath);
      setKeyMessage(t("ext.publish.keyRestored"));
      setRestorePath(null);
    } catch (reason) {
      setKeyError(String(reason));
    } finally {
      setKeyBusy(false);
    }
  };

  return (
    <section className="space-y-4 border-t border-border pt-6">
      <h2 className="text-[18px] font-semibold text-foreground">{t("ext.publish.title")}</h2>
      <div className="space-y-2 border-b border-border pb-4">
        <h3 className="text-[14px] font-semibold">{t("ext.publish.keyTitle")}</h3>
        <p className="text-[12.5px] text-muted-foreground">{t("ext.publish.keyHelp")}</p>
        <div className="flex gap-2">
          <Button size="sm" variant="secondary" disabled={keyBusy} onClick={() => void backupKey()}>{t("ext.publish.keyBackup")}</Button>
          <Button size="sm" variant="secondary" disabled={keyBusy} onClick={() => void chooseRestore()}>{t("ext.publish.keyRestore")}</Button>
        </div>
        {keyMessage && <p className="text-[12px] text-muted-foreground">{keyMessage}</p>}
        {keyError && <p className="text-[12px] text-destructive">{keyError}</p>}
      </div>
      {!check && !done && !error && (
        <div data-drop-target="ext-publish" className="rounded-md border border-dashed border-border p-5 text-center">
          <p className="text-[12.5px] text-muted-foreground">{t("ext.publish.drop")}</p>
          <div className="mt-3 flex justify-center gap-2">
            <Button size="sm" variant="secondary" onClick={() => void choose(false)}>{t("ext.publish.chooseFile")}</Button>
            <Button size="sm" variant="secondary" onClick={() => void choose(true)}>{t("ext.publish.chooseFolder")}</Button>
          </div>
        </div>
      )}
      {busy && <p className="text-[12.5px] text-muted-foreground">{t("ext.publish.checking")}</p>}
      {error && (
        <div className="rounded-md border border-border p-3 text-[12.5px]">
          <p>{error}</p>
          <Button className="mt-2" size="sm" variant="secondary" onClick={() => { setError(""); setPath(null); }}>
            {t("ext.publish.another")}
          </Button>
        </div>
      )}
      {check && (
        <div className="space-y-2 text-[12.5px]">
          {check.preview.thumbnail && (
            <img className="h-24 rounded border border-border" src={check.preview.thumbnail} alt="" />
          )}
          <div className="font-semibold">{check.preview.name} · {check.preview.version}</div>
          <div className="text-muted-foreground">{check.preview.author}</div>
          <p>{check.preview.summary}</p>
          <dl className="space-y-2">
            <DetailRow label={t("ext.categories")} value={check.preview.categories.join(", ")} />
            <DetailRow
              label={t("ext.permissions")}
              value={check.preview.permissions.map((permission) => labels[permission as Permission]
                ? t(labels[permission as Permission])
                : permission).join(", ") || t("ext.noPermissions")}
            />
            {check.preview.network.length > 0 && (
              <DetailRow label={t("ext.networkHosts")} value={check.preview.network.join(", ")} />
            )}
            <DetailRow label={t("ext.size")} value={`${(check.preview.size / 1024 / 1024).toFixed(2)} MB`} />
          </dl>
          <p>{check.outcome === "listed"
            ? t("ext.publish.willCommunity")
            : check.reviewReason === "new-permissions"
              ? t("ext.publish.willPermissions")
              : t("ext.publish.willVerifiedUpdate")}</p>
          {check.signedWith === "author-key" && <p>{t("ext.publish.authorKey")}</p>}
          <div className="flex justify-end gap-2">
            <Button size="sm" variant="secondary" onClick={() => { setCheck(null); setPath(null); }}>
              {t("ext.publish.cancel")}
            </Button>
            <Button size="sm" onClick={() => void publish()}>{t("ext.publish.publish")}</Button>
          </div>
        </div>
      )}
      {done && (
        <div className="text-[12.5px]">
          <p>{done === "listed" ? t("ext.publish.live") : t("ext.publish.sent")}</p>
          {verificationRequested && <p className="mt-1 text-muted-foreground">{t("ext.publish.inReview", { version: doneVersion })}</p>}
          <div className="mt-2 flex justify-end gap-2">
            <Button size="sm" variant="secondary" onClick={reset}>{t("ext.publish.publishAnother")}</Button>
            {done === "listed" && doneTier === "community" && doneId && !verificationRequested && (
              <Button size="sm" onClick={() => void verify(doneId, "done")}>{t("ext.publish.request")}</Button>
            )}
          </div>
          {verifyError?.target === "done" && verifyError.id === doneId && (
            <p className="mt-1 text-muted-foreground">{verifyError.message}</p>
          )}
        </div>
      )}
      {mine.length > 0 && (
        <div className="space-y-2 border-t border-border pt-4">
          <h3 className="text-[14px] font-semibold">{t("ext.publish.yours")}</h3>
          {mine.map((listing) => (
            <div key={listing.id} className="text-[12.5px]">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div>{listing.name} · {listing.version}</div>
                  <div className="text-muted-foreground">{status(listing)}</div>
                </div>
                {!listing.verified && listing.review?.state !== "pending" && (
                  <Button size="sm" variant="secondary" onClick={() => void verify(listing.id, "listing")}>
                    {t("ext.publish.request")}
                  </Button>
                )}
              </div>
              {verifyError?.target === "listing" && verifyError.id === listing.id && (
                <p className="mt-1 text-muted-foreground">{verifyError.message}</p>
              )}
            </div>
          ))}
        </div>
      )}
      <AlertDialog open={restorePath !== null} onOpenChange={(open) => !open && setRestorePath(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("ext.publish.keyRestoreTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("ext.publish.keyRestoreBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={keyBusy}>{t("ext.publish.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={keyBusy} onClick={() => void restoreKey()}>{t("ext.publish.keyReplace")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-3">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="text-right text-foreground/85">{value}</dd>
    </div>
  );
}
