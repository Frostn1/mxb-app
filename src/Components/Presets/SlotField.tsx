import { cn } from "@/lib/utils";
import { Input } from "../ui/input";
import type { SlotDef } from "../../lib/presets";

/**
 * One editable customization slot: an input with a datalist of installed options
 * (so unknown / captured values and free-text fonts still work), plus a "missing
 * mod" badge. Shared by the Presets builder and the Rider render studio.
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
  const listId = `slot-${slot.key}`;
  return (
    <label className="flex flex-col gap-1">
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
      <Input
        list={options.length ? listId : undefined}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Stock"
        className={cn("h-8 text-[12.5px]", missing && "border-amber-500/40")}
      />
      {options.length > 0 && (
        <datalist id={listId}>
          {options.map((o) => (
            <option key={o} value={o} />
          ))}
        </datalist>
      )}
    </label>
  );
}
