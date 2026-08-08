import type React from "react";
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
  onChange,
}: {
  slot: SlotDef;
  value: string;
  options: string[];
  missing: boolean;
  /** Shown under the field — says why it's empty when "No matches." wouldn't. */
  hint?: React.ReactNode;
  onChange: (v: string) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-1">
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
      <Combobox value={value} options={options} onChange={onChange} invalid={missing} />
      {hint && <span className="text-[11px] leading-snug text-faint">{hint}</span>}
    </div>
  );
}
