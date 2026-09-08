import { cn } from "@/lib/utils";

interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
}

/** A small controlled toggle: a leaning plate with a square knob, like the buttons. */
export function Switch({ checked, onCheckedChange, disabled, className }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "u-skew relative h-5 w-9 shrink-0 cursor-default transition-colors disabled:opacity-50",
        checked ? "bg-primary" : "bg-foreground/15",
        className,
      )}
    >
      <span
        className={cn(
          "absolute top-[3px] h-[14px] w-[14px] transition-all",
          checked ? "right-[3px] bg-primary-foreground" : "left-[3px] bg-foreground/70",
        )}
      />
    </button>
  );
}
