import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
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
import { Switch } from "@frost/shared/Components/ui/switch";
import { useT } from "@/i18n";
import { writePaintProxy, type ProxyTarget } from "./proxyExport";

/**
 * Asks the one thing a painting proxy leaves to the creator — whether their model's own look
 * goes out with it — then where to put it. Open while `target` is set.
 */
export function ProxyDialog({ target, onClose }: { target: ProxyTarget | null; onClose: () => void }) {
  const t = useT();
  const [shading, setShading] = useState(false);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    if (!target) return;
    const picked = await openDialog({ directory: true });
    const dir = Array.isArray(picked) ? picked[0] : picked;
    if (!dir) return;
    setBusy(true);
    try {
      const res = await writePaintProxy(target, dir, shading);
      toast.success(t("designer.exportedProxy", { dir }), {
        description: t("designer.exportedProxyDesc", { kept: res.triangles, of: res.sourceTriangles }),
      });
      onClose();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AlertDialog open={!!target} onOpenChange={(open) => !open && !busy && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("designer.proxyTitle", { name: target?.name ?? "" })}</AlertDialogTitle>
          <AlertDialogDescription>{t("designer.exportProxyHint")}</AlertDialogDescription>
        </AlertDialogHeader>
        {target?.shading && (
          <label className="flex cursor-pointer items-center justify-between gap-4 text-[13px]">
            <span>
              <span className="font-medium">{t("designer.proxyShading")}</span>
              <span className="block text-muted-foreground">{t("designer.proxyShadingHint")}</span>
            </span>
            <Switch checked={shading} onCheckedChange={setShading} disabled={busy} />
          </label>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy}
            onClick={(e) => {
              // Stays open through the folder pick and the write; closes itself when done.
              e.preventDefault();
              void run();
            }}
          >
            {busy ? t("designer.proxyWriting") : t("designer.proxyChoose")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
