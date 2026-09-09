import { cn } from "@/lib/utils";

export interface SegmentedOption<T extends string> {
  value: T;
  label: React.ReactNode;
}

interface SegmentedProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  className?: string;
  size?: "sm" | "md";
}

/** A compact segmented control (the design's Tracks/Bikes + theme toggles). */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  className,
  size = "md",
}: SegmentedProps<T>) {
  return (
    <div
      className={cn(
        "inline-flex gap-0.5 rounded-lg border border-input bg-card p-[3px]",
        className,
      )}
    >
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            className={cn(
              "cursor-default rounded-md font-semibold transition-colors",
              size === "sm" ? "px-3.5 py-1 text-[11.5px]" : "px-4 py-[5px] text-[12.5px]",
              active
                ? "bg-secondary text-foreground"
                : "font-medium text-muted-foreground hover:text-foreground",
            )}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
