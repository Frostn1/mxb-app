import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { SOURCE_LABELS, type ModSource } from "./sources";

interface SourceFilterProps {
  /** Only the sources this build and this game can actually reach. */
  options: ModSource[];
  value: ModSource;
  onChange: (source: ModSource) => void;
}

/**
 * Where a mod comes from, as a filter rather than as a place.
 *
 * A source that cannot answer is not offered: a build with no shop credential, or a title
 * whose `caps.shop` is off, gets a shorter control rather than a segment that opens an empty
 * grid. When only one source is left there is no choice to make, and the control disappears.
 */
export default function SourceFilter({ options, value, onChange }: SourceFilterProps) {
  const t = useT();
  if (options.length < 2) return null;

  return (
    <div
      role="radiogroup"
      // The bar's left slot is `items-stretch` so a tab can draw a full-height underline. A
      // fixed-height item in that row aligns to the top instead of stretching, which reads as
      // the control sitting high; centre it explicitly.
      className="flex h-7 shrink-0 items-center gap-0.5 self-center rounded-full border border-input bg-card p-0.5"
    >
      {options.map((source) => {
        const on = source === value;
        return (
          <button
            key={source}
            role="radio"
            aria-checked={on}
            onClick={() => onChange(source)}
            className={cn(
              "h-6 cursor-default rounded-full px-2.5 font-cond text-[11.5px] font-semibold tracking-[-0.01em] transition-colors",
              on
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {t(SOURCE_LABELS[source])}
          </button>
        );
      })}
    </div>
  );
}
