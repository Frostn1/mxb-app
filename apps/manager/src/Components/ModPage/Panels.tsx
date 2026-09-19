import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";

/** A card in the right-hand column, with the label that says what it is. */
export function Panel({
  label,
  children,
  className,
}: {
  label?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-2.5 rounded-xl border border-border bg-card p-4",
        className,
      )}
    >
      {label && <SectionLabel>{label}</SectionLabel>}
      {children}
    </div>
  );
}

export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <span className="font-cond text-[10.5px] font-bold uppercase tracking-[0.14em] text-faint">
      {children}
    </span>
  );
}

/** One line of the fact list. A row with no value is never rendered — see {@link Facts}. */
export interface Fact {
  label: string;
  value?: string | null | false;
  /** Path-ish values, set in the monospace face and allowed to wrap. */
  mono?: boolean;
}

/**
 * The facts a source actually carries.
 *
 * A row with nothing in it is dropped, not rendered empty. The library's nine-row grid had
 * four rows blank on most mods, which read as missing data rather than as data a `.pkz`
 * simply doesn't carry; a shorter list of things that are true says more.
 */
export function Facts({ rows }: { rows: Fact[] }) {
  const shown = rows.filter((r): r is Fact & { value: string } => !!r.value);
  if (shown.length === 0) return null;
  return (
    <div className="flex flex-col gap-2">
      {shown.map((r) => (
        <div key={r.label} className="flex items-start justify-between gap-4 text-[12px]">
          <span className="flex-none text-muted-foreground">{r.label}</span>
          <span
            className={cn(
              "min-w-0 text-right text-foreground/85",
              r.mono
                ? "select-text break-all font-mono text-[11px] text-muted-foreground"
                : "truncate",
            )}
          >
            {r.value}
          </span>
        </div>
      ))}
    </div>
  );
}

/** A tinted line of prose — a hint, a warning, a notice. */
export function Note({
  icon: Icon,
  tone = "muted",
  children,
}: {
  icon?: LucideIcon;
  tone?: "muted" | "success" | "warning" | "primary" | "destructive";
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex items-start gap-2.5 rounded-xl border px-3.5 py-2.5 text-[12px] leading-relaxed",
        tone === "muted" && "border-border bg-foreground/[0.03] text-muted-foreground",
        tone === "success" && "border-success/25 bg-success/[0.06] text-success/90",
        tone === "warning" && "border-warning/30 bg-warning/[0.07] text-warning/90",
        tone === "primary" && "border-primary/25 bg-primary/[0.06] text-muted-foreground",
        tone === "destructive" && "border-destructive/40 bg-destructive/[0.08] text-destructive",
      )}
    >
      {Icon && <Icon className="mt-0.5 size-3.5 flex-none" />}
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

export interface InsideItem {
  key: string;
  label: string;
  /** A short right-hand note: a file count, a size, a variant name. */
  hint?: string;
  icon?: LucideIcon;
  onClick?: () => void;
}

export interface InsideGroup {
  key: string;
  label: string;
  items: InsideItem[];
}

/**
 * What the file holds — a track's layouts, a bike's model swaps, liveries and sounds.
 *
 * Shown before you own it wherever the source can say, because "you cannot tell what is in
 * the download until it is installed" was the complaint. Where a source genuinely cannot
 * say, the caller passes nothing and the block disappears: an empty "What's inside" would be
 * a worse answer than no answer.
 */
export function WhatsInside({ groups }: { groups: InsideGroup[] }) {
  const shown = groups.filter((g) => g.items.length > 0);
  if (shown.length === 0) return null;
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4">
      <SectionLabel>What&rsquo;s inside</SectionLabel>
      {shown.map((g) => (
        <div key={g.key} className="flex flex-col gap-1.5">
          <span className="font-cond text-[11px] font-semibold tracking-[-0.01em] text-muted-foreground">
            {g.label} · <span className="tabular-figures">{g.items.length}</span>
          </span>
          <div className="flex flex-col gap-1">
            {g.items.map((it) => {
              const Icon = it.icon;
              const Tag = it.onClick ? "button" : "div";
              return (
                <Tag
                  key={it.key}
                  onClick={it.onClick}
                  className={cn(
                    "flex items-center gap-2.5 rounded-lg border border-border bg-background/40 px-3 py-2 text-left",
                    it.onClick && "cursor-default transition-colors hover:border-primary/40",
                  )}
                >
                  {Icon && <Icon className="size-3.5 flex-none text-faint" />}
                  <span className="min-w-0 flex-1 truncate text-[12.5px]">{it.label}</span>
                  {it.hint && (
                    <span className="flex-none font-cond text-[11px] tabular-figures text-faint">
                      {it.hint}
                    </span>
                  )}
                </Tag>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
