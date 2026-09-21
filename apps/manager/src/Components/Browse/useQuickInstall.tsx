import { useCallback, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { resolveQuickInstall, type ModType } from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import type { ModSummary } from "@frost/shared/types";
import { useInstall } from "../../Context/Install";
import type { InstalledIndex } from "../../lib/installedMatch";
import { useT } from "@/i18n";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@frost/shared/Components/ui/alert-dialog";

/**
 * Queue a mxb-mods listing for install, with the "you already have this" guard.
 *
 * Lifted out of `Browse` because the merged Mods grid offers the same action on the same
 * listings — two copies of the resolve-late logic and the reinstall dialog would drift the
 * moment either was touched. `Browse` keeps the selection and the bulk bar; this is only the
 * part both grids share.
 */
export function useQuickInstall(
  modType: ModType,
  categoryId: number,
  installed: InstalledIndex,
) {
  const t = useT();
  const { game } = useConfig();
  const { startPendingInstall } = useInstall();
  // A pending reinstall the user must confirm (they already have these mods).
  const [reinstall, setReinstall] = useState<
    | { kind: "single"; mod: ModSummary }
    | { kind: "bulk"; mods: ModSummary[]; onQueued?: () => void }
    | null
  >(null);

  const isInstalled = useCallback((mod: ModSummary) => installed.has(mod.title), [installed]);

  /**
   * Silent quick-install: the mod joins the download queue on the spot, and its mirror and
   * destination folder are worked out when the queue reaches it.
   *
   * The lookup used to come first, which meant a mod existed nowhere on screen until its page
   * came back — and a bulk selection of twenty appeared in the panel a page-fetch at a time.
   * Resolving late also asks the library where the mod should go *after* the installs ahead of
   * it have landed, which is the state the answer depends on.
   */
  const queue = useCallback(
    (mod: ModSummary) =>
      startPendingInstall({
        slug: mod.slug,
        title: mod.title,
        subpath: modType.installSubpath,
        resolve: async () => {
          try {
            const res = await resolveQuickInstall(mod.slug, modType, game, categoryId);
            if (res.ok) return { ...res.params, categoryId };
            if (res.reason === "blocked") {
              toast.error(t("browse.needsBrowser", { title: res.title }), {
                description: t("browse.needsBrowserDesc", { host: res.host ?? "" }),
              });
            } else if (res.reason === "serverOnly") {
              // Not installed on the user's behalf: one click can't ask which build was meant,
              // and a server file installs cleanly while the game shows nothing.
              toast.error(t("browse.serverOnly", { title: res.title }), {
                description: t("browse.serverOnlyDesc"),
              });
            } else {
              toast.error(t("browse.noDownload", { title: res.title }));
            }
          } catch (e) {
            toast.error(t("browse.quickInstallFailed", { title: mod.title }), {
              description: String(e),
            });
          }
          return null;
        },
      }),
    [modType, categoryId, game, startPendingInstall, t],
  );

  const doOne = useCallback(
    (mod: ModSummary) => {
      queue(mod);
      toast.success(t("browse.queued", { title: mod.title }), {
        description: t("browse.queuedDesc"),
      });
    },
    [queue, t],
  );

  // The whole selection is queued at once. Nothing is fetched here, so there's no busy state
  // to sit through and no count of what got skipped — a mod with no usable download says so
  // itself, by name, when the queue gets to it.
  const doMany = useCallback(
    (list: ModSummary[]) => {
      list.forEach(queue);
      toast.success(t("browse.queuedBulk", { count: list.length }), {
        description: t("browse.queuedBulkDesc"),
      });
    },
    [queue, t],
  );

  /** One mod, confirming first when it is already on disk. */
  const quickInstall = useCallback(
    (mod: ModSummary) => {
      if (isInstalled(mod)) setReinstall({ kind: "single", mod });
      else doOne(mod);
    },
    [isInstalled, doOne],
  );

  /** A selection, confirming first when any of them are already on disk. */
  const quickInstallMany = useCallback(
    (list: ModSummary[], onQueued?: () => void) => {
      if (list.some(isInstalled)) setReinstall({ kind: "bulk", mods: list, onQueued });
      else {
        doMany(list);
        onQueued?.();
      }
    },
    [isInstalled, doMany],
  );

  const confirm = useCallback(() => {
    const pending = reinstall;
    setReinstall(null);
    if (!pending) return;
    if (pending.kind === "single") doOne(pending.mod);
    else {
      doMany(pending.mods);
      pending.onQueued?.();
    }
  }, [reinstall, doOne, doMany]);

  const dialog: ReactNode = (
    <AlertDialog open={Boolean(reinstall)} onOpenChange={(o) => !o && setReinstall(null)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {reinstall?.kind === "single"
              ? t("browse.reinstallOne", { title: reinstall.mod.title })
              : t("browse.reinstallMany")}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {reinstall?.kind === "single"
              ? t("browse.reinstallOneBody")
              : t("browse.reinstallManyBody", {
                  installed: reinstall?.mods.filter(isInstalled).length ?? 0,
                  total: reinstall?.mods.length ?? 0,
                })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={confirm}>
            {reinstall?.kind === "single" ? t("browse.reinstall") : t("browse.reinstallAll")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );

  return { quickInstall, quickInstallMany, isInstalled, dialog };
}
