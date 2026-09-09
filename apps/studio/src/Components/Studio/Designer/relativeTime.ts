/**
 * "2 days ago" from unix seconds, without a date library.
 *
 * Deliberately coarse: the list is already ordered by recency, so the number only has to say
 * roughly how stale something is. `Intl.RelativeTimeFormat` does the wording in whatever
 * locale is running, so this adds no strings of its own.
 */
const STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["second", 60],
  ["minute", 60],
  ["hour", 24],
  ["day", 7],
  ["week", 4.345],
  ["month", 12],
  ["year", Number.POSITIVE_INFINITY],
];

export function relativeTime(unixSeconds: number): string {
  const fmt = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  let value = Math.round(Date.now() / 1000 - unixSeconds);
  if (value < 45) return fmt.format(0, "second");
  for (const [unit, span] of STEPS) {
    if (Math.abs(value) < span) return fmt.format(-Math.round(value), unit);
    value /= span;
  }
  return fmt.format(-Math.round(value), "year");
}
