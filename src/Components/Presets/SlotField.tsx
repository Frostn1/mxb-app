import type React from "react";
import { cn } from "@/lib/utils";
import { Combobox } from "../ui/combobox";
import type { SlotDef } from "../../lib/presets";
import { useT } from "../../i18n/context";

/**
 * One editable customization slot: a searchable **creatable** combobox over the
 * installed options — click to see them all, type to filter, or commit a free-text
 * value (fonts, a captured mod name not currently installed) — plus a "missing mod"
 * badge. Shared by the Presets builder and the Rider render studio.
 */
export function SlotField({
  slot,
  value,
  options,
  missing,
  hint,
  compact,
  row,
  onChange,
}: {
  slot: SlotDef;
  value: string;
  options: string[];
  missing: boolean;
  /** Shown under the field — says why it's empty when "No matches." wouldn't. */
  hint?: React.ReactNode;
  /** Tighter rows, for a screen showing a dozen of these next to a 3D render. */
  compact?: boolean;
  /** The mockup's slot row: a bordered line with the slot name over its value, for a
   *  narrow column standing beside the model rather than a form filling the window. */
  row?: boolean;
  onChange: (v: string) => void;
}) {
  const t = useT();
  if (row) {
    return (
      <div
        className={cn(
          "flex flex-col border bg-card px-3 py-1.5",
          missing ? "border-warning/40" : "border-border",
        )}
      >
        <span className="flex items-center gap-1.5 font-cond text-[10.5px] font-semibold uppercase tracking-[0.2em] text-faint">
          {t(slot.label)}
          {missing && (
            <span title={t("presets.missingHint")} className="text-warning">
              {t("presets.missing")}
            </span>
          )}
        </span>
        <Combobox
          value={value}
          options={options}
          onChange={onChange}
          invalid={missing}
          className="h-6 border-0 bg-transparent px-0 text-[12.5px]"
        />
        {hint && <span className="pb-1 text-[11px] leading-snug text-faint">{hint}</span>}
      </div>
    );
  }
  return (
    <div className={cn("flex flex-col", compact ? "gap-0.5" : "gap-1")}>
      <span className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
        {t(slot.label)}
        {missing && (
          <span
            title={t("presets.missingHint")}
            className="rounded bg-amber-500/15 px-1 text-[9.5px] font-semibold uppercase text-amber-500"
          >
            {t("presets.missing")}
          </span>
        )}
      </span>
      <Combobox
        value={value}
        options={options}
        onChange={onChange}
        invalid={missing}
        className={compact ? "h-7 text-[12px]" : undefined}
      />
      {hint && <span className="text-[11px] leading-snug text-faint">{hint}</span>}
    </div>
  );
}
