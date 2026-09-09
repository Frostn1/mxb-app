import type { ReactNode } from "react";
import { cn } from "@frost/shared/lib/utils";

export interface RailEntry<T extends string> {
  id: T;
  label: string;
}

/**
 * The studio's navigation: the tools, named, down the left.
 *
 * Not the manager's horizontal rail — a tool row across the top costs the canvas its full
 * height and puts the thing you are working on second. And not an icon strip either: seven
 * glyphs for seven tools meant two near-identical people for Rider and Pose and two
 * near-identical padlocks for Protect and Secure, which is what an icon per row gets you once
 * the rows stop being nouns you can draw. The names are shorter to read than a glyph you have
 * to decode, and they are what people call these tools anyway.
 *
 * The active mark is the ink colour plus a hairline in the accent — the least a list can do
 * and still be unambiguous.
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
    <nav className="flex w-[148px] shrink-0 flex-col border-r border-border bg-window px-3 py-3.5">
      {header}
      <div className="mt-6 flex flex-1 flex-col items-stretch gap-px">
        {entries.map((e) => (
          <RailButton
            key={e.id}
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
  label,
  on,
  onClick,
}: {
  label: string;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      aria-current={on ? "page" : undefined}
      className={cn(
        "relative cursor-default rounded-[3px] px-2.5 py-[7px] text-left text-[13px] transition-colors",
        on
          ? "bg-foreground/[0.06] font-medium text-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {on && <span className="absolute inset-y-[6px] left-0 w-[2px] bg-primary" />}
      {label}
    </button>
  );
}
