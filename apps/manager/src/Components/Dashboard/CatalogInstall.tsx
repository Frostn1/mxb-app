import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  catalogDownload,
  catalogInstallType,
  getCatalogMod,
  pickCatalogFile,
  type CatalogMod,
} from "@frost/shared/api/catalog";
import { isBlockedDownload, quickDestination } from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { formatBytes } from "@frost/shared/lib/mods";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@frost/shared/Components/ui/alert-dialog";
import { useInstall } from "../../Context/Install";
import { useT } from "@/i18n";

/** The mxb-mods slug of a mirrored post (`https://mxb-mods.com/<slug>/`), so the queue and the
 *  history name it the way Browse does. An upload has none and keeps the catalog's own. */
function installSlug(mod: CatalogMod): string {
  const m = mod.source_url ? /^https:\/\/mxb-mods\.com\/([^/?#]+)\/?$/i.exec(mod.source_url) : null;
  return m ? m[1] : mod.slug;
}

/**
 * The prompt an `mxb://install?id=<uuid>` link opens: the catalog mod's name, and Install.
 *
 * A link is something any web page can open, so nothing downloads until the player says so.
 * Once they do, the mod joins the ordinary install queue: our CDN copy when it is stored,
 * otherwise the copy is asked for and waited on, and the original host only if that fails.
 */
export function CatalogInstall({ link, onClose }: { link: { id: string } | null; onClose: () => void }) {
  const t = useT();
  const { game } = useConfig();
  const { startPendingInstall } = useInstall();
  const [mod, setMod] = useState<CatalogMod | null>(null);

  useEffect(() => {
    setMod(null);
    if (!link) return;
    let cancelled = false;
    getCatalogMod(link.id)
      .then((m) => {
        if (cancelled) return;
        if (!catalogInstallType(m.type) || !pickCatalogFile(m.files)) {
          toast.error(t("browse.noDownload", { title: m.title }));
          onClose();
        } else setMod(m);
      })
      .catch(() => {
        if (cancelled) return;
        toast.error(t("catalogInstall.notFound"));
        onClose();
      });
    return () => {
      cancelled = true;
    };
  }, [link, onClose, t]);

  const kind = mod ? catalogInstallType(mod.type) : null;
  const file = mod ? pickCatalogFile(mod.files) : null;

  const install = () => {
    if (!mod || !kind || !file) return;
    const { modType, categoryId } = kind;
    const slug = installSlug(mod);
    startPendingInstall({
      slug,
      title: mod.title,
      subpath: modType.installSubpath,
      resolve: async () => {
        const [dest, download] = await Promise.all([
          quickDestination(game, modType, categoryId, mod.title, mod.categories),
          catalogDownload(mod, file, { wait: true }),
        ]);
        if (!download) {
          toast.error(t("browse.noDownload", { title: mod.title }));
          return null;
        }
        if (isBlockedDownload(download)) {
          toast.error(t("browse.needsBrowser", { title: mod.title }), {
            description: t("browse.needsBrowserDesc", { host: download.host }),
          });
          return null;
        }
        return {
          slug,
          title: mod.title,
          subpath: modType.installSubpath,
          destFolder: dest.destFolder,
          categoryId,
          url: download.url,
          host: download.host,
        };
      },
    });
    toast.success(t("browse.queued", { title: mod.title }));
    onClose();
  };

  const size = file?.size ? formatBytes(file.size) : "";
  return (
    <AlertDialog open={Boolean(mod)} onOpenChange={(o) => !o && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("catalogInstall.title", { title: mod?.title ?? "" })}</AlertDialogTitle>
          <AlertDialogDescription>
            {[kind ? t(kind.modType.label) : "", size].filter(Boolean).join(" · ")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction onClick={install}>{t("catalogInstall.install")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
