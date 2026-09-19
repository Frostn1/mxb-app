import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import SmartImg from "./Img";

/**
 * The row that carries the mod and the one thing you came here to do.
 *
 * Fixed under the context bar and outside every scroller, because the primary action used to
 * move: a tall button inside a scrolling card on Browse, a different card in the shop, and the
 * fourth of six identical outline buttons in the library. Here it is always the same control
 * in the same place, whichever door you came in, and scrolling the page never takes it away.
 */
export function ActionBar({
  image,
  fallbackIcon: Fallback,
  title,
  meta,
  children,
}: {
  /** The mod's own picture, at thumbnail size. */
  image: string | null;
  /** Drawn in place of the picture when the source has none. */
  fallbackIcon?: LucideIcon;
  title: string;
  /** "Track · Author · v1.2" — anything empty is dropped rather than left as a stray dot. */
  meta: (string | null | undefined | false)[];
  /** The state chip and the primary action, right-aligned. */
  children?: ReactNode;
}) {
  const parts = meta.filter((m): m is string => !!m && m.trim() !== "");
  return (
    <div className="flex h-[60px] flex-none items-center gap-3.5 border-b border-border bg-window px-7">
      <div className="grid size-10 flex-none place-items-center overflow-hidden rounded-lg border border-border bg-card text-foreground/25">
        {image ? (
          <SmartImg src={image} width={120} alt="" className="size-full object-cover" />
        ) : Fallback ? (
          <Fallback className="size-4" strokeWidth={1.5} />
        ) : null}
      </div>

      <div className="flex min-w-0 flex-col justify-center">
        <h1 className="truncate font-cond text-[15px] font-extrabold leading-tight tracking-[-0.045em]">
          {title}
        </h1>
        {parts.length > 0 && (
          <span className="truncate font-cond text-[11.5px] leading-tight text-muted-foreground">
            {parts.join(" · ")}
          </span>
        )}
      </div>

      <div className="ml-auto flex flex-none items-center gap-2.5">{children}</div>
    </div>
  );
}

/** Where this mod stands with you: in your library, owned, locked. */
export function StateChip({
  icon: Icon,
  tone = "muted",
  children,
}: {
  icon?: LucideIcon;
  tone?: "success" | "primary" | "muted";
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        "flex h-7 items-center gap-1.5 rounded-full px-3 font-cond text-[11.5px] font-semibold tracking-[-0.01em]",
        tone === "success" && "bg-success/[0.12] text-success",
        tone === "primary" && "bg-primary/[0.14] text-primary",
        tone === "muted" && "bg-foreground/[0.07] text-muted-foreground",
      )}
    >
      {Icon && <Icon className="size-3.5" strokeWidth={2.5} />}
      {children}
    </span>
  );
}
