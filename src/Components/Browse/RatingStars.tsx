import { Star } from "lucide-react";
import type { ModRating } from "../../types";

/** mxb-mods.com prints one decimal ("3.7"), and drops the ".0" on whole numbers. */
function formatAverage(average: number): string {
  return average.toFixed(1).replace(/\.0$/, "");
}

/**
 * The site's own rating readout: five stars filled to the nearest half, then the average
 * and the vote count. Rendered as a gold layer clipped over a dim one — a half star is a
 * clip at 50%, so there's no separate half-star glyph to keep aligned with the full one.
 */
export default function RatingStars({ rating }: { rating: ModRating }) {
  const rounded = Math.round(Math.min(rating.average, 5) * 2) / 2;
  const stars = Array.from({ length: 5 });

  return (
    <span
      className="flex items-center gap-1 text-[10.5px] font-semibold text-white/85"
      title={`${formatAverage(rating.average)} out of 5 — ${rating.count} ${
        rating.count === 1 ? "rating" : "ratings"
      } on mxb-mods.com`}
    >
      <span className="relative inline-flex">
        <span className="flex gap-px text-white/25">
          {stars.map((_, i) => (
            <Star key={i} className="size-[11px]" fill="currentColor" strokeWidth={0} />
          ))}
        </span>
        <span
          className="absolute inset-y-0 left-0 flex gap-px overflow-hidden text-[#ffc531]"
          style={{ width: `${(rounded / 5) * 100}%` }}
          aria-hidden
        >
          {stars.map((_, i) => (
            <Star
              key={i}
              className="size-[11px] flex-none"
              fill="currentColor"
              strokeWidth={0}
            />
          ))}
        </span>
      </span>
      {formatAverage(rating.average)}
      <span className="font-normal text-white/60">({rating.count})</span>
    </span>
  );
}
