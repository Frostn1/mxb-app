import type { ReactNode } from "react";
import { ChevronLeft } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";

/** A scrolling page with a title row: an optional back link, a subtitle and actions. */
export default function Page({
  title,
  sub,
  onBack,
  backLabel,
  actions,
  wide,
  children,
}: {
  title: ReactNode;
  sub?: ReactNode;
  onBack?: () => void;
  backLabel?: string;
  actions?: ReactNode;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="h-full overflow-y-auto">
      <div className={cn("mx-auto px-8 py-8", wide ? "max-w-[1400px]" : "max-w-3xl")}>
        {onBack && (
          <button
            onClick={onBack}
            className="mb-3 flex items-center gap-1 text-[12px] text-muted-foreground hover:text-foreground"
          >
            <ChevronLeft className="size-3.5" />
            {backLabel}
          </button>
        )}
        <div className="flex items-end justify-between gap-4">
          <div className="min-w-0">
            <h2 className="truncate font-cond text-[20px] font-semibold tracking-[0.03em]">{title}</h2>
            {sub && <div className="mt-1 text-[12.5px] text-muted-foreground">{sub}</div>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
        </div>
        <div className="mt-6">{children}</div>
      </div>
    </div>
  );
}

/** A small uppercase heading over a block. */
export function Label({ children }: { children: ReactNode }) {
  return (
    <div className="mb-2 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
      {children}
    </div>
  );
}
