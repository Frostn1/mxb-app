import { Check, Loader2, Palette } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import { useT } from "@/i18n";
import type { usePaintSyncSetup } from "../../lib/usePaintSyncSetup";

/**
 * The short confirm before "Turn on paint sync": what it does, and every switch it is about
 * to flip. Paint sync shares the rider's paints with the server, and may install Game
 * Integration on the way, so neither happens without the player reading the list first.
 */
export default function PaintSyncConfirm({
  open,
  onOpenChange,
  setup,
  onDone,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The caller's `usePaintSyncSetup()`, so the dialog and the card agree on one state. */
  setup: ReturnType<typeof usePaintSyncSetup>;
  /** After it's on. */
  onDone?: () => void;
}) {
  const t = useT();
  const { steps, busy, turnOn } = setup;

  const confirm = async () => {
    const ok = await turnOn();
    if (ok) {
      onOpenChange(false);
      onDone?.();
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !busy && onOpenChange(next)}>
      <DialogContent className="max-w-[460px] gap-5">
        <div className="flex items-start gap-3">
          <div className="grid size-10 flex-none place-items-center rounded-xl bg-primary/10 text-primary">
            <Palette className="size-5" />
          </div>
          <div className="flex min-w-0 flex-col gap-1.5">
            <DialogTitle className="text-[18px] leading-tight">
              {t("paintSync.confirmTitle")}
            </DialogTitle>
            <DialogDescription>{t("paintSync.what")}</DialogDescription>
          </div>
        </div>

        <div className="flex flex-col gap-2 rounded-xl border border-input bg-foreground/[0.025] p-3.5">
          <span className="text-[11.5px] font-semibold text-muted-foreground">
            {t("paintSync.confirmList")}
          </span>
          {steps.map((s) => (
            <div key={s} className="flex items-start gap-2 text-[12.5px] text-foreground/85">
              <Check className="mt-[2px] size-3.5 flex-none text-primary" strokeWidth={3} />
              {t(s)}
            </div>
          ))}
          <span className="pt-1 text-[11.5px] leading-relaxed text-muted-foreground">
            {t("paintSync.confirmNote")}
          </span>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
            {t("paintSync.notNow")}
          </Button>
          <Button onClick={() => void confirm()} disabled={busy}>
            {busy && <Loader2 className="size-4 animate-spin" />}
            {t("paintSync.turnOn")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
