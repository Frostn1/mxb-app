import { Search, X } from "lucide-react";
import { cn } from "../../lib/utils";

/**
 * The search box every list uses.
 *
 * There were seven of these, hand-rolled, and they had drifted: three widths, two of them
 * rounded and the rest square, one with a clear button and the others without. A control
 * that appears on every screen has to be the same control on every screen — so it is one
 * component, and a screen that needs a different width says so rather than restating the
 * whole thing.
 */
export function SearchBox({
  value,
  onChange,
  placeholder,
  className,
  autoFocus,
  clearLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Width, and nothing else: `w-[210px]`, or `flex-1` where the bar can give it the room. */
  className?: string;
  autoFocus?: boolean;
  /** Title for the clear button. Absent hides it — some lists search as you type over a
   *  short list, where an empty box is one keystroke away anyway. */
  clearLabel?: string;
}) {
  return (
    <div
      className={cn(
        "flex h-7 min-w-[116px] items-center gap-2 rounded-full border border-input bg-card px-3",
        className,
      )}
    >
      <Search className="size-3.5 flex-none text-faint" />
      <input
        value={value}
        autoFocus={autoFocus}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full min-w-0 bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
      />
      {clearLabel && value && (
        <button
          type="button"
          onClick={() => onChange("")}
          title={clearLabel}
          aria-label={clearLabel}
          className="flex-none cursor-default rounded-full p-0.5 text-faint transition-colors hover:text-foreground"
        >
          <X className="size-3" />
        </button>
      )}
    </div>
  );
}
