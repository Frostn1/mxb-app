import { useEffect, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../ui/alert-dialog";
import { useT } from "../../i18n/context";
import { uninstallApp, uninstallInfo, type UninstallInfo } from "../../api/uninstall";
import { canSelfRemove, confirmMatches, dataChoice } from "../../lib/uninstall";

/**
 * "Uninstall" for the Settings page of each app, shared so all three remove themselves the
 * same way. It reads its own state, like `SurveySetting`.
 *
 * The removal itself is the platform's (see `crates/core/src/uninstall.rs`). This asks twice:
 * once to open the dialog, once by typing the app's name. "Also delete my data" is off unless
 * turned on, and isn't offered for a folder another installed app still reads.
 */
export default function UninstallSetting() {
  const t = useT();
  const [info, setInfo] = useState<UninstallInfo | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let stopped = false;
    uninstallInfo()
      .then((i) => {
        if (!stopped) setInfo(i);
      })
      .catch(() => {});
    return () => {
      stopped = true;
    };
  }, []);

  if (!info) return null;
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">
          {t("uninstall.label", { app: info.product })}
        </span>
        <span className="text-[11.5px] leading-relaxed text-muted-foreground">
          {canSelfRemove(info) ? t("uninstall.desc") : t("uninstall.manual")}
        </span>
        {info.method === "package" && info.command && (
          <div className="mt-1 flex items-center gap-2">
            <code className="select-text rounded bg-background px-2 py-1 font-mono text-[11.5px]">
              {info.command}
            </code>
            <Button
              size="sm"
              variant="outline"
              className="h-6 px-2 text-xs"
              onClick={() => void navigator.clipboard?.writeText(info.command ?? "")}
            >
              {t("uninstall.copy")}
            </Button>
          </div>
        )}
      </div>
      {canSelfRemove(info) && (
        <Button size="sm" variant="destructive" className="flex-none" onClick={() => setOpen(true)}>
          {t("uninstall.button")}
        </Button>
      )}
      <UninstallDialog info={info} open={open} onOpenChange={setOpen} />
    </div>
  );
}

interface DialogProps {
  info: UninstallInfo;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Injected for tests; defaults to the real command. */
  run?: (deleteData: boolean) => Promise<void>;
}

/** The confirm step: what happens, the opt-in for data, and the typed name. */
export function UninstallDialog({ info, open, onOpenChange, run = uninstallApp }: DialogProps) {
  const t = useT();
  const [typed, setTyped] = useState("");
  const [deleteData, setDeleteData] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const data = dataChoice(info);

  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      // On success the app exits and this never returns.
      await run(deleteData && data.offered);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const what =
    info.method === "trash"
      ? t("uninstall.whatTrash", { app: info.product })
      : info.method === "delete"
        ? t("uninstall.whatDelete", { app: info.product })
        : t("uninstall.whatLaunch", { app: info.product });

  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (busy) return;
        if (!next) {
          setTyped("");
          setDeleteData(false);
          setError(null);
        }
        onOpenChange(next);
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("uninstall.title", { app: info.product })}</AlertDialogTitle>
          <AlertDialogDescription>{what}</AlertDialogDescription>
        </AlertDialogHeader>

        <p className="text-[12px] leading-relaxed text-muted-foreground">{t("uninstall.kept")}</p>

        <div className="flex items-start justify-between gap-4 rounded-lg border border-border p-3">
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[12.5px] text-foreground/85">{t("uninstall.deleteData")}</span>
            <span className="text-[11.5px] leading-relaxed text-muted-foreground">
              {data.blockedBy.length > 0
                ? t("uninstall.deleteDataShared", { apps: data.blockedBy.join(", ") })
                : t("uninstall.deleteDataDesc")}
            </span>
            {data.offered && deleteData && (
              <ul className="mt-1 select-text font-mono text-[11px] text-faint">
                {info.dataDirs.map((d) => (
                  <li key={d} className="truncate" title={d}>
                    {d}
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div className="pt-0.5">
            <Switch
              checked={deleteData && data.offered}
              disabled={!data.offered || busy}
              onCheckedChange={setDeleteData}
            />
          </div>
        </div>

        {info.nsis && (
          <p className="text-[11.5px] leading-relaxed text-muted-foreground">
            {data.blockedBy.length > 0
              ? t("uninstall.nsisShared", { apps: data.blockedBy.join(", ") })
              : t("uninstall.nsisNote")}
          </p>
        )}

        <label className="flex flex-col gap-1.5 text-[12px] text-foreground/85">
          {t("uninstall.typeName", { app: info.product })}
          <Input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={info.product}
            disabled={busy}
            autoFocus
          />
        </label>

        {error && <p className="select-text text-[12px] text-destructive">{error}</p>}

        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>{t("uninstall.cancel")}</AlertDialogCancel>
          <Button
            variant="destructive"
            disabled={busy || !confirmMatches(typed, info.product)}
            onClick={() => void go()}
          >
            {busy ? t("uninstall.working") : t("uninstall.confirm", { app: info.product })}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
