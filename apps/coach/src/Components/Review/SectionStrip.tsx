import { cn } from "@frost/shared/lib/utils";
import type { Review } from "@/api/coach";
import { gap } from "@/lib/format";
import { shortName } from "./TrackMap";

/** Red for time lost, green for time gained, stronger the more it is. */
function tint(lost: number, most: number): string {
  const k = Math.round(25 + 60 * Math.min(1, Math.abs(lost) / Math.max(most, 0.05)));
  if (lost > 0.05) return `color-mix(in srgb, var(--destructive) ${k}%, transparent)`;
  if (lost < -0.05) return `color-mix(in srgb, var(--success) ${k}%, transparent)`;
  return "var(--secondary)";
}

/** The lap as one bar, start to finish: a block per section, sized by its length. */
export default function SectionStrip({
  review,
  selected,
  onPick,
}: {
  review: Review;
  selected: number | null;
  onPick: (i: number) => void;
}) {
  const most = Math.max(...review.sections.map((s) => Math.abs(s.lost)));
  const total = Math.max(1, review.sections[review.sections.length - 1]?.end ?? 1);
  return (
    <div className="flex h-10 w-full gap-[2px]">
      {review.sections.map((s, i) => {
        const share = (s.end - s.start) / total;
        return (
          <button
            key={i}
            onClick={() => onPick(i)}
            title={`${s.name}  ${gap(s.lost)} s`}
            style={{ flexGrow: s.end - s.start, flexBasis: 0, background: tint(s.lost, most) }}
            className={cn(
              "relative min-w-[6px] rounded-[3px] text-[10.5px] font-semibold text-foreground/80 transition-transform hover:brightness-125",
              selected === i && "z-10 scale-y-[1.18] outline outline-2 outline-foreground",
            )}
          >
            {share > 0.035 && shortName(s.name)}
          </button>
        );
      })}
    </div>
  );
}
