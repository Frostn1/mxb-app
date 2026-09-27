import { useState } from "react";
import { CheckCircle2, FolderOpen, Loader2, RefreshCw, Undo2, XCircle } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";
import { useT } from "@/i18n";
import type { useBlenderStatus } from "./useBlenderStatus";

/**
 * Blender's status: one small control in the title bar's own slot, not a bar the Bike tab
 * used to draw above everything else in it — the same status text, and a "Blender ▾" pill
 * that opened the same detail, were two ways to read one fact stacked on top of each other.
 * This is the one control: the icon and short text ARE the button; its own popover holds the
 * detail (the full path, choosing blender.exe, forgetting a saved one, checking again).
 */
export default function BlenderBar({ blender }: { blender: ReturnType<typeof useBlenderStatus> }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const { status, checking, ready, summary, check, onPickBlender, onForgetBlender } = blender;
  const found = status?.found ?? null;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          title={summary}
          className="flex max-w-[11rem] items-center gap-1.5 rounded-md px-1.5 py-1 text-[12px] text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground"
        >
          {status === null ? (
            <Loader2 className="size-3.5 shrink-0 animate-spin text-faint" />
          ) : ready ? (
            <CheckCircle2 className="size-3.5 shrink-0 text-success" />
          ) : (
            <XCircle className="size-3.5 shrink-0 text-amber-500" />
          )}
          <span className="min-w-0 truncate">{summary}</span>
        </button>
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
  );
}
