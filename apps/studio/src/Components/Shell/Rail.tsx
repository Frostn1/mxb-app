import type { ComponentType, ReactNode } from "react";
import { cn } from "@frost/shared/lib/utils";

export interface RailEntry<T extends string> {
  id: T;
  label: string;
  icon: ComponentType<{ className?: string }>;
}

/**
 * The studio's navigation: a slim column of tools down the left.
 *
 * Not the manager's horizontal rail, and not for the sake of being different — a tool row
 * across the top costs the canvas its full height and puts the thing you are working on
 * second. Seven tools that each own the whole window want an edge, and the edge with the
 * least to lose is the narrow one. The label sits under the icon rather than in a tooltip,
 * because a tool you use for an hour should not need hovering to identify.
 *
 * The active mark is a filled tile rather than the manager's skewed edge bar: at this size a
 * 2px line on the boundary between the rail and the canvas is a mark you have to look for.
 */
export default function Rail<T extends string>({
  entries,
  active,
  onPick,
  header,
  footer,
}: {
  entries: RailEntry<T>[];
  active: T;
  onPick: (id: T) => void;
  header?: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <nav className="flex w-[84px] shrink-0 flex-col items-center gap-1 border-r border-border bg-window px-2.5 py-3">
      {header}
      <div className="mt-4 flex flex-1 flex-col items-stretch gap-1 self-stretch">
        {entries.map((e) => (
          <RailButton
            key={e.id}
            icon={e.icon}
            label={e.label}
            on={e.id === active}
            onClick={() => onPick(e.id)}
          />
        ))}
      </div>
      {footer}
    </nav>
  );
}

export function RailButton({
  icon: Icon,
  label,
  on,
  onClick,
}: {
  icon: ComponentType<{ className?: string }>;
  label: string;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-current={on ? "page" : undefined}
      className={cn(
        "group flex cursor-default flex-col items-center gap-1.5 rounded-lg px-1 py-2.5 transition-colors",
        on
          ? "bg-foreground/[0.09] text-foreground"
          : "text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
      )}
    >
      <Icon className="size-[19px]" />
      <span className="text-[9.5px] font-medium uppercase tracking-[0.08em]">{label}</span>
    </button>
  );
}
