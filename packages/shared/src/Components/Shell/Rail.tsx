import { Fragment, type ReactNode } from "react";
import { cn } from "../../lib/utils";

export interface RailEntry<T extends string> {
  id: T;
  label: string;
  /** Entries sharing a group sit together; a rule separates one group from the next. */
  group?: string;
}

/**
 * The app's mark at the top of the rail: the snowflake, and a two-line logotype — the small
 * italic word over the name, stepped in under it so the two overlap and read as one mark.
 * Draggable, since the rail is one of the few places to grab a frameless window.
 */
export function RailBrand({
  top,
  name,
  logo = "/logo.svg",
}: {
  top: string;
  name: string;
  logo?: string;
}) {
  return (
    <div data-tauri-drag-region className="flex select-none items-center gap-2.5 px-2.5 pt-0.5">
      {/* The app's own mark, not a lettered plate — the same two-paint snowflake the icon and
          the installer carry. */}
      <img
        src={logo}
        alt=""
        draggable={false}
        className="size-[28px] flex-none [filter:drop-shadow(0_1px_3px_rgba(0,0,0,0.25))]"
      />
      {/* The only face in the app that is not Barlow. */}
      <span className="flex min-w-0 flex-col items-start">
        <span className="font-serif text-[15px] italic leading-none text-muted-foreground">
          {top}
        </span>
        <span className="-mt-[5px] ml-[15px] font-serif text-[19px] font-bold italic leading-none tracking-[-0.01em] text-foreground">
          {name}
        </span>
      </span>
    </div>
  );
}

/**
 * Named navigation down the left, shared by the Studio and Coach.
 *
 * In the Studio: the tools, named, down the left.
 *
 * Not the manager's horizontal rail — a tool row across the top costs the canvas its full
 * height and puts the thing you are working on second. And not an icon strip either: one
 * glyph per tool meant two near-identical people for Rider and Pose, which is what an icon
 * per row gets you once the rows stop being nouns you can draw. The names are quicker to read than a glyph
 * you have to decode, and they are what people call these tools anyway.
 *
 * Grouped, because "make something" and "check something already on disk" are different
 * errands and a flat list says they are the same one.
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
    <nav className="flex w-[152px] shrink-0 flex-col border-r border-border bg-window px-2.5 py-3.5">
      {header}
      <div className="mt-6 flex flex-1 flex-col items-stretch gap-0.5">
        {entries.map((e, i) => (
          <Fragment key={e.id}>
            {i > 0 && e.group !== entries[i - 1].group && (
              <hr className="my-2 border-0 border-t border-border" />
            )}
            <RailButton
              label={e.label}
              on={e.id === active}
              onClick={() => onPick(e.id)}
            />
          </Fragment>
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
        "relative cursor-default rounded-[4px] px-2.5 py-[7px] text-left text-[13px] transition-colors",
        on
          ? "bg-foreground/[0.07] font-medium text-foreground"
          : "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground",
      )}
    >
      {on && <span className="absolute inset-y-[6px] left-0 w-[2px] rounded-full bg-primary" />}
      {label}
    </button>
  );
}
