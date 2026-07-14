import { cn } from "@/lib/utils";

interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
}

/** A small controlled toggle matching the design's pill switch. */
export function Switch({ checked, onCheckedChange, disabled, className }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "relative h-5 w-9 shrink-0 cursor-default rounded-full transition-colors disabled:opacity-50",
        checked ? "bg-primary" : "bg-foreground/15",
        className,
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 size-4 rounded-full transition-all",
          checked ? "right-0.5 bg-primary-foreground" : "left-0.5 bg-foreground/70",
        )}
      />
    </button>
  );
}
