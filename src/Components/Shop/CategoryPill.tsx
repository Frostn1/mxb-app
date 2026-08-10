import { cn } from "@/lib/utils";

/** One filter pill, shared by both halves of the Shop so the two rows can't drift. */
export default function CategoryPill({
  label,
  count,
  on,
  small,
  onClick,
}: {
  label: string;
  count?: number;
  on: boolean;
  small?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "cursor-default rounded-full transition-colors",
        small ? "px-3 py-[3px] text-[11.5px]" : "px-3.5 py-[5px] text-[12px]",
        "font-medium",
        on
          ? "bg-foreground font-semibold text-background"
          : "border border-input text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
      {count !== undefined && count > 0 && (
        <span className={cn("ml-1.5", on ? "text-background/60" : "text-faint")}>
          {count}
        </span>
      )}
    </button>
  );
}
