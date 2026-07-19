import { Combobox } from "../ui/combobox";
import type { SlotDef } from "../../lib/presets";

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
  onChange,
}: {
  slot: SlotDef;
  value: string;
  options: string[];
  missing: boolean;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
        {slot.label}
        {missing && (
          <span
            title="This mod isn't installed — shows as stock in-game"
            className="rounded bg-amber-500/15 px-1 text-[9.5px] font-semibold uppercase text-amber-500"
          >
            missing
          </span>
        )}
      </span>
      <Combobox value={value} options={options} onChange={onChange} invalid={missing} />
    </div>
  );
}
