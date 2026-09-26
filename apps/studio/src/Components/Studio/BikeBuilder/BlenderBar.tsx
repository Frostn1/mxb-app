import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, ChevronDown, FolderOpen, Loader2, RefreshCw, Undo2, XCircle } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@frost/shared/Components/ui/popover";
import { useT } from "@/i18n";
import { blenderStatus, setBlenderPath, type BlenderStatus } from "../../../api/bikebuild";

/**
 * Blender's status, as one line instead of a card that used to sit above every mode
 * (Assemble and Part Maker both) — Sean's "the file picker is right inside all of the other
 * tabs" was this: whichever part of the bike builder a rider opened, the choose-blender.exe
 * button was already there, above everything else. It only matters once, and only until
 * Blender is found, so it's a status line with the details a click away, not a fixture.
 */
export default function BlenderBar({ onReadyChange }: { onReadyChange?: (ready: boolean) => void }) {
  const t = useT();
  const [status, setStatus] = useState<BlenderStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [open, setOpen] = useState(false);

  const check = useCallback(() => {
    setChecking(true);
    blenderStatus()
      .then(setStatus)
      .catch((e) => toast.error(t("bike.blenderCheckFailed"), { description: String(e) }))
      .finally(() => setChecking(false));
  }, [t]);
  useEffect(() => check(), [check]);
  useEffect(() => onReadyChange?.(!!status?.found?.supported), [status, onReadyChange]);

  async function onPickBlender() {
    const file = await openDialog({ multiple: false, filters: [{ name: "Blender", extensions: ["exe"] }] });
    if (typeof file !== "string") return;
    setChecking(true);
    try {
      const next = await setBlenderPath(file);
      setStatus(next);
      if (!next.found || next.found.path.toLowerCase() !== file.toLowerCase()) toast.error(t("bike.notBlender"));
    } catch (e) {
      toast.error(t("bike.notBlender"), { description: String(e) });
    } finally {
      setChecking(false);
    }
  }

  async function onForgetBlender() {
    setChecking(true);
    try {
      setStatus(await setBlenderPath(""));
    } finally {
      setChecking(false);
    }
  }

  const found = status?.found ?? null;
  const ready = !!found?.supported;
  const summary =
    status === null
      ? t("bike.looking")
      : ready
        ? t("bike.blenderFound", { version: found!.version })
        : found
          ? t("bike.blenderTooOld", { version: found.version, min: status.minVersion })
          : t("bike.blenderMissing", { min: status.minVersion });

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-window px-4 py-1.5 text-[12px]">
      {status === null ? (
        <Loader2 className="size-3.5 shrink-0 animate-spin text-faint" />
      ) : ready ? (
        <CheckCircle2 className="size-3.5 shrink-0 text-success" />
      ) : (
        <XCircle className="size-3.5 shrink-0 text-amber-500" />
      )}
      <span className="min-w-0 flex-1 truncate text-muted-foreground">{summary}</span>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button size="sm" variant="ghost" className="h-6 gap-1 px-1.5 text-[11px]">
            {t("bike.blender")}
            <ChevronDown className="size-3" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="end" className="flex w-72 flex-col gap-2 text-sm">
          {found && <p className="truncate font-mono text-[11px] text-muted-foreground">{found.path}</p>}
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onClick={onPickBlender} disabled={checking}>
              <FolderOpen className="size-3.5" />
              {t("bike.chooseBlender")}
            </Button>
            {status?.saved && (
              <Button size="sm" variant="ghost" onClick={onForgetBlender} disabled={checking}>
                <Undo2 className="size-3.5" />
                {t("bike.findBlender")}
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={check} disabled={checking}>
              {checking ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
            </Button>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}
