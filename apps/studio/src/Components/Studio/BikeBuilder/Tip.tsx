import { Tooltip, TooltipContent, TooltipTrigger } from "@frost/shared/Components/ui/tooltip";
import { cn } from "@frost/shared/lib/utils";

/** A key, or keys, drawn the way the Blender keys help draws them. */
export function Kbd({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center rounded border border-border bg-muted/60 px-1 font-mono text-[10.5px] font-medium text-muted-foreground",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/**
 * The Bike tab's tooltip: what a control does and, when it has one, its key — so the keys are
 * learnt from the buttons, as Blender teaches its own. Wraps its child in a span so a disabled
 * button still shows why (a disabled button gets no pointer events of its own).
 */
export function Tip({
  label,
  keys,
  side = "bottom",
  children,
  className,
}: {
  label: React.ReactNode;
  keys?: string;
  side?: "top" | "bottom" | "left" | "right";
  children: React.ReactNode;
  className?: string;
}) {
  if (!label) return <>{children}</>;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className={cn("inline-flex min-w-0", className)}>{children}</span>
      </TooltipTrigger>
      <TooltipContent side={side} className="flex max-w-[22rem] items-center gap-2">
        <span className="min-w-0">{label}</span>
        {keys && <Kbd className="shrink-0">{keys}</Kbd>}
      </TooltipContent>
    </Tooltip>
  );
}
