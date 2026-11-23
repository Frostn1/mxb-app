import { useCallback, useState } from "react";
import { AlertTriangle, X } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { openSettings } from "../../../lib/openSettings";
import type { useBlenderStatus } from "./useBlenderStatus";

/** Which action last asked for Blender and didn't get it — the message shows at that one. */
export type BlenderAction = "add" | "split" | "refresh" | "base" | "build" | "make";

/**
 * Blender, asked for only where it's needed. There's no standing "Blender wasn't found" row
 * any more: a rider who never adds a .blend never needs to hear about it. An action that
 * runs Blender calls `need(action)` first; when Blender's ready that's just `true`, and when
 * it isn't, it's `false` and that action's own spot shows a short line pointing at
 * Settings → Paths (where choosing blender.exe now lives).
 */
export function useBlenderGate(blender: ReturnType<typeof useBlenderStatus>) {
  const [asked, setAsked] = useState<BlenderAction | null>(null);
  const need = useCallback(
    (action: BlenderAction) => {
      // Still looking: let the action wait for the answer rather than flash a warning that
      // a moment later turns out to be wrong. The buttons are disabled meanwhile anyway.
      if (blender.status === null) return false;
      if (blender.ready) {
        setAsked(null);
        return true;
      }
      setAsked(action);
      return false;
    },
    [blender.status, blender.ready],
  );
  return { asked, need, dismiss: () => setAsked(null), blender };
}

export default function BlenderNeeded({
  gate,
  at,
  className,
}: {
  gate: ReturnType<typeof useBlenderGate>;
  /** Shows only when this is the action that asked. */
  at: BlenderAction | BlenderAction[];
  className?: string;
}) {
  const t = useT();
  const ats = Array.isArray(at) ? at : [at];
  if (!gate.asked || !ats.includes(gate.asked)) return null;
  const { status } = gate.blender;
  const tooOld = !!status?.found && !status.found.supported;
  return (
    <p
      role="status"
      className={cn("flex min-w-0 items-center gap-1.5 text-[12px] text-amber-500", className)}
    >
      <AlertTriangle className="size-3.5 shrink-0" />
      <span className="min-w-0 truncate">
        {tooOld ? t("bike.needsNewerBlender", { min: status!.minVersion }) : t("bike.needsBlender")}{" "}
        <button
          type="button"
          onClick={() => {
            gate.dismiss();
            openSettings("paths");
          }}
          className="font-medium text-primary underline-offset-2 hover:underline"
        >
          {t("bike.settingsPaths")}
        </button>
      </span>
      <button
        type="button"
        onClick={gate.dismiss}
        title={t("common.close")}
        className="shrink-0 text-faint hover:text-foreground"
      >
        <X className="size-3" />
      </button>
    </p>
  );
}
