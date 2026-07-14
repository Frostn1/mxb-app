import { cn } from "@/lib/utils";

interface ProgressProps {
  /** 0–100. Omit for an indeterminate bar. */
  value?: number;
  className?: string;
  barClassName?: string;
}

function Progress({ value, className, barClassName }: ProgressProps) {
  const indeterminate = value === undefined;
  return (
    <div
      className={cn(
        "relative h-1 overflow-hidden rounded-full bg-foreground/[0.08]",
        className,
      )}
    >
      <div
        className={cn(
          "h-full rounded-full bg-primary transition-[width] duration-300",
          indeterminate && "w-1/3 animate-[frost-indeterminate_1.2s_ease-in-out_infinite]",
          barClassName,
        )}
        style={indeterminate ? undefined : { width: `${Math.max(0, Math.min(100, value))}%` }}
      />
    </div>
  );
}

export { Progress };
